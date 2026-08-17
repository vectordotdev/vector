use crate::config::{LogNamespace, SourceContext};
use crate::event::{Event, LogEvent};
use crate::internal_events::{
    EventsReceived, OdbcFailedError, OdbcQueryExecuted, StreamClosedError,
};
use crate::shutdown::ShutdownSignal;
use crate::sinks::prelude::*;
use crate::sources::odbc::config::{OdbcConfig, OdbcStatementParam};
use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
use chrono_tz::Tz;
use futures::pin_mut;
use futures_util::StreamExt;
use odbc_api::buffers::{AnySlice, BufferDesc, ColumnarAnyBuffer};
use odbc_api::parameter::VarCharBox;
use odbc_api::{
    ConnectionOptions, Cursor, CursorRow, DataType, Environment, IntoParameter, ResultSetMetadata,
    environment,
};
use snafu::{ResultExt, Snafu};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::mem;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant};
use tokio::select;
use vector_common::internal_event::{
    ByteSize, BytesReceived, ComponentEventsDropped, CountByteSize, InternalEventHandle as _,
    Protocol, Registered, UNINTENTIONAL,
};
use vector_common::json_size::JsonSize;
use vector_lib::EstimatedJsonEncodedSizeOf;
use vector_lib::emit;
use vector_lib::source_sender::{SendError, chunk_size_events};
use vrl::prelude::*;

const TIMESTAMP_FORMATS: &[&str] = &[
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%dT%H:%M:%S",
    "%Y/%m/%d %H:%M:%S",
    "%Y/%m/%dT%H:%M:%S",
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y/%m/%d %H:%M:%S%.f",
    "%Y/%m/%dT%H:%M:%S%.f",
];

struct Column {
    column_name: String,
    column_type: DataType,
}

/// Columns of the query result.
type Columns = Vec<Column>;
/// Rows of the query result.
type Rows = Vec<Value>;

#[derive(Debug, Snafu)]
pub enum OdbcError {
    #[snafu(display("ODBC database error: {source}"))]
    Db { source: odbc_api::Error },

    #[snafu(display("File IO error: {source}"))]
    Io { source: std::io::Error },

    #[snafu(display("Send error: {source}"))]
    SendError { source: SendError },

    #[snafu(display("JSON error: {source}"))]
    Json { source: serde_json::Error },

    #[snafu(display("Blocking ODBC task failed: {source}"))]
    BlockingTask { source: tokio::task::JoinError },

    #[snafu(display("Missing tracking column `{column}`"))]
    MissingTrackingColumn { column: String },

    #[snafu(display(
        "Tracking column `{column}` has a value that cannot be converted to an ODBC parameter"
    ))]
    InvalidTrackingValue { column: String },

    #[snafu(display("Last query result row is not an object; cannot extract tracking columns"))]
    InvalidTrackingRow,

    #[snafu(display("Query result row is not an object; cannot convert to a log event"))]
    InvalidResultRow,

    #[snafu(display(
        "Query returned duplicate column names: {columns:?}; alias columns in the SQL statement"
    ))]
    DuplicateColumnNames { columns: Vec<String> },

    /// Pipeline send failed after the tracking checkpoint was already committed.
    ///
    /// The schedule must still advance `prev_params` with `next_params` so in-memory
    /// tracking (no `last_run_metadata_path`) does not re-emit rows already handed off.
    /// `SendError::Closed` is terminal for the schedule loop; `SendError::Timeout` remains
    /// retryable on the next tick.
    #[snafu(display("Send failed after tracking checkpoint was committed: {source}"))]
    SendFailedAfterCheckpoint {
        source: SendError,
        next_params: Vec<OdbcStatementParam>,
        /// Events permanently skipped after the checkpoint was committed.
        dropped_events: usize,
    },

    /// Shutdown interrupted delivery after an on-disk tracking checkpoint was committed.
    #[snafu(display(
        "Shutdown interrupted delivery after tracking checkpoint was committed ({dropped_events} events dropped)"
    ))]
    ShutdownAfterCheckpoint { dropped_events: usize },

    #[snafu(display("ODBC source shutting down"))]
    Shutdown,
}

pub(crate) struct Context {
    cfg: OdbcConfig,
    env: &'static Environment,
    cx: SourceContext,
    log_namespace: LogNamespace,
}

impl Context {
    pub(crate) fn new(
        cfg: OdbcConfig,
        cx: SourceContext,
        log_namespace: LogNamespace,
    ) -> Result<Self, OdbcError> {
        let env = environment().context(DbSnafu)?;

        Ok(Self {
            cfg,
            env,
            cx,
            log_namespace,
        })
    }

    pub(crate) async fn run_schedule(self: Box<Self>) -> Result<(), ()> {
        let shutdown = self.cx.shutdown.clone();

        let schedule = self.cfg.schedule.clone().stream(self.cfg.schedule_timezone);
        pin_mut!(schedule);

        let bytes_received = register!(BytesReceived::from(Protocol::from("odbc")));
        let events_received = register!(EventsReceived);

        #[cfg(test)]
        let mut count = 0;

        let mut prev_params = self.cfg.statement_init_params.clone();

        loop {
            select! {
                _ = shutdown.clone() => {
                    debug!(message = "Shutdown signal received. Shutting down ODBC source.");
                    break;
                }
                next = schedule.next() => {
                    if next.is_none() {
                        debug!(message = "Schedule exhausted. Shutting down ODBC source.");
                        break;
                    }

                    let instant = Instant::now();
                    match self
                        .process(
                            prev_params.clone(),
                            &bytes_received,
                            &events_received,
                            shutdown.clone(),
                        )
                        .await
                    {
                        Ok(result) => {
                            // Cache the overlaid param list for runs without an on-disk
                            // checkpoint. When `last_run_metadata_path` exists, later ticks
                            // reload tracking from disk onto the config template instead.
                            if result.is_some() {
                                prev_params = result;
                            }

                            emit!(OdbcQueryExecuted {
                                statement: &self.cfg.statement.clone().unwrap_or_default(),
                                elapsed: instant.elapsed().as_millis(),
                            });
                        }
                        Err(OdbcError::Shutdown) => {
                            debug!(
                                message =
                                    "Shutdown signal received during ODBC query. Shutting down ODBC source."
                            );
                            break;
                        }
                        Err(OdbcError::SendFailedAfterCheckpoint {
                            source,
                            next_params,
                            dropped_events,
                        }) => {
                            // Checkpoint was committed before emit; advance in-memory overlay
                            // so the next tick does not replay rows already handed to the pipeline.
                            prev_params = Some(next_params);
                            if dropped_events > 0 {
                                emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                                    count: dropped_events,
                                    reason: "ODBC tracking checkpoint was committed before downstream delivery failed.",
                                });
                            }
                            // Closed is terminal: SourceSender never reopens. Timeout stays
                            // retryable on the next schedule tick.
                            let closed = matches!(source, SendError::Closed);
                            emit!(OdbcFailedError {
                                statement: &self.cfg.statement.clone().unwrap_or_default(),
                                error: OdbcError::SendError { source },
                            });
                            if closed {
                                break;
                            }
                        }
                        Err(OdbcError::ShutdownAfterCheckpoint { dropped_events }) => {
                            if dropped_events > 0 {
                                emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                                    count: dropped_events,
                                    reason: "ODBC tracking checkpoint was committed before shutdown completed downstream delivery.",
                                });
                            }
                            debug!(
                                message =
                                    "Shutdown signal received after ODBC tracking checkpoint was committed. Shutting down ODBC source."
                            );
                            break;
                        }
                        Err(error) => {
                            // Closed is terminal for the same reason as above; other errors
                            // (including send timeout) remain retryable.
                            let closed = matches!(
                                error,
                                OdbcError::SendError {
                                    source: SendError::Closed
                                }
                            );
                            emit!(OdbcFailedError {
                                statement: &self.cfg.statement.clone().unwrap_or_default(),
                                error,
                            });
                            if closed {
                                break;
                            }
                        }
                    }

                    #[cfg(test)]
                    {
                        count += 1;
                        if let Some(iterations) = self.cfg.iterations
                            && count >= iterations {
                                debug!(message = "No additional schedule configured. Shutting down ODBC source.");
                                break;
                            }
                    }
                }
            }
        }

        Ok(())
    }

    /// Executes the scheduled ODBC query and sends the result as events in bounded batches.
    ///
    /// When `tracking_columns` is set, batches are buffered until the query finishes, the
    /// final-row checkpoint is validated, overlaid onto `statement_init_params`, persisted,
    /// and only then are events sent downstream. That preserves at-most-once tracking
    /// semantics: a missing/unbindable tracking value fails the poll before any pipeline
    /// emit (avoiding infinite replay), while a send failure after a successful checkpoint
    /// save may skip those rows on the next run. A query or conversion failure after batches
    /// were received emits `ComponentEventsDropped` for the buffered rows and does not
    /// commit a checkpoint. When `last_run_metadata_path` is unset, the in-memory overlay
    /// is still advanced after a post-checkpoint send failure so already-sent rows are not
    /// replayed.
    ///
    /// Without tracking, batches are streamed to the pipeline as they arrive.
    ///
    /// Shutdown closes the batch channel so the blocking fetch stops on the next send, then
    /// waits for the blocking task. Connect/execute still depend on `login_timeout` /
    /// `statement_timeout`; with either set to `0`, that wait can block until the driver returns.
    async fn process(
        &self,
        params: Option<Vec<OdbcStatementParam>>,
        bytes_received: &Registered<BytesReceived>,
        events_received: &Registered<EventsReceived>,
        mut shutdown: ShutdownSignal,
    ) -> Result<Option<Vec<OdbcStatementParam>>, OdbcError> {
        let conn_str = self.cfg.connection_string_or_file().context(IoSnafu)?;
        let stmt_str = self.cfg.statement_or_file().context(IoSnafu)?;
        if stmt_str.trim().is_empty() {
            return Err(OdbcError::Io {
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "either a non-empty `statement` or a readable `statement_filepath` must be provided",
                ),
            });
        }
        let out = self.cx.out.clone();
        let env = self.env;

        // Prefer on-disk tracking overlays when available. Otherwise bind the in-memory
        // parameter list (config template, then prior run with tracking values overlaid).
        // Unreadable or corrupt metadata is treated as an error to avoid replaying old rows.
        //
        // When a checkpoint exists it is the tracking SSOT and is overlaid onto the config
        // template (`prev_params` unused). Without a checkpoint, `current` is bound as-is.
        let tz = self.cfg.odbc_default_timezone;
        let template = self
            .cfg
            .statement_init_params
            .as_deref()
            .unwrap_or_default();
        let current = params.as_deref().unwrap_or(template);
        let tracking_columns = self.cfg.tracking_columns.as_deref();
        let tracking_enabled = tracking_columns.is_some_and(|columns| !columns.is_empty());
        let overlay = self
            .cfg
            .last_run_metadata_path
            .as_deref()
            .map(load_tracking_map)
            .transpose()?
            .flatten();
        let (base, overlay) = match overlay.as_ref() {
            // Checkpoint overlays keep static template values intact.
            Some(overlay) => (template, Some(overlay)),
            None => (current, None),
        };
        let stmt_params = order_params(base, overlay, tracking_columns, tz)?;
        let cfg = self.cfg.clone();
        let login_timeout = cfg.login_timeout;
        let statement_timeout = cfg.statement_timeout;
        let batch_size = cfg.odbc_batch_size;
        let max_str_limit = (cfg.odbc_max_str_limit > 0).then_some(cfg.odbc_max_str_limit);

        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let blocking = tokio::task::spawn_blocking(move || {
            let result = execute_query(
                env,
                &conn_str,
                &stmt_str,
                stmt_params,
                login_timeout,
                statement_timeout,
                tz,
                batch_size,
                max_str_limit,
                |batch| {
                    if tx.blocking_send(batch).is_err() {
                        return Ok(false);
                    }
                    Ok(true)
                },
            );
            drop(tx);
            result
        });

        // With tracking enabled, hold converted batches and the final row until the query
        // completes so checkpoint validation can run before any pipeline emit.
        // Received-event metrics are recorded at conversion time (before enrichment /
        // buffering / send) so checkpoint, shutdown, or downstream failures cannot hide
        // already-created input.
        let mut pending_batches = Vec::new();
        let mut final_row = None;
        let mut stream_error = None;

        loop {
            let batch_rows = select! {
                _ = &mut shutdown => {
                    return shutdown_query(rx, blocking).await;
                }
                batch = rx.recv() => batch,
            };

            let Some(batch_rows) = batch_rows else {
                break;
            };
            if batch_rows.is_empty() {
                continue;
            }

            // Keep the last raw row for tracking before rows are moved into events.
            if tracking_enabled {
                final_row = batch_rows.last().cloned();
            }
            let mut events = match rows_to_events(batch_rows) {
                Ok(result) => result,
                Err(error) => {
                    stream_error = Some(error);
                    break;
                }
            };

            if events.is_empty() {
                continue;
            }

            // Count/size unenriched events immediately after creation, matching
            // ComponentEventsReceived (docs/specs/component.md).
            let event_count = events.len();
            let byte_size = events.estimated_json_encoded_size_of();
            record_received(bytes_received, events_received, event_count, byte_size);

            self.enrich_events(&mut events);

            if tracking_enabled {
                pending_batches.push(events);
                continue;
            }

            match send_enriched_batch(&out, events, &mut shutdown).await {
                Ok(()) => {}
                Err(BatchSendError::Shutdown { .. }) => {
                    // Remaining chunks are re-read after restart; in-flight is via UnsentEventCount.
                    return shutdown_query(rx, blocking).await;
                }
                Err(BatchSendError::Send {
                    source,
                    dropped_events,
                }) => {
                    // Closed: emit chunks never handed to SourceSender (in-flight via StreamClosedError).
                    // Timeout is retryable — do not discard.
                    if dropped_events > 0 && matches!(source, SendError::Closed) {
                        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                            count: dropped_events,
                            reason: "Source stream closed before remaining ODBC batch chunks were sent.",
                        });
                    }
                    stream_error = Some(OdbcError::SendError { source });
                    break;
                }
            }
        }

        drop(rx);
        let join_result = blocking.await.context(BlockingTaskSnafu);

        if let Some(error) = stream_error {
            // Prefer the stream conversion/send error over a later join outcome.
            drop(join_result);
            emit_discarded_tracking_batches(
                &pending_batches,
                "ODBC result conversion failed after tracking batches were buffered.",
            );
            return Err(error);
        }

        match join_result {
            Ok(Ok(())) => {}
            Ok(Err(error)) | Err(error) => {
                emit_discarded_tracking_batches(
                    &pending_batches,
                    "ODBC query failed after tracking batches were buffered.",
                );
                return Err(error);
            }
        }

        // Tracking path: validate + overlay + persist the final-row checkpoint before any
        // pipeline emit so a missing/null tracking value cannot replay forever, and an
        // overlay failure cannot leave a persisted checkpoint that would skip unsent rows.
        let next_params = match (final_row, cfg.tracking_columns.as_ref()) {
            (Some(last), Some(tracking_columns)) => Some(prepare_tracking_checkpoint(
                cfg.last_run_metadata_path.as_deref(),
                last,
                template,
                tracking_columns,
                tz,
            )?),
            _ => None,
        };

        let mut pending_batches = pending_batches.into_iter();
        while let Some(events) = pending_batches.next() {
            match send_enriched_batch(&out, events, &mut shutdown).await {
                Ok(()) => {}
                Err(BatchSendError::Shutdown { dropped_events }) => {
                    let dropped_events = dropped_events
                        + pending_batches
                            .as_slice()
                            .iter()
                            .map(Vec::len)
                            .sum::<usize>();

                    // An on-disk checkpoint prevents these rows from being replayed after
                    // restart, so they must be accounted for as dropped. Without one, a
                    // restart re-reads the rows and this shutdown does not lose them.
                    return if cfg.last_run_metadata_path.is_some() {
                        Err(OdbcError::ShutdownAfterCheckpoint { dropped_events })
                    } else {
                        Err(OdbcError::Shutdown)
                    };
                }
                Err(BatchSendError::Send {
                    source,
                    dropped_events,
                }) => {
                    // Checkpoint is already committed. Advance in-memory tracking on send
                    // failure so the next tick cannot replay rows already emitted.
                    return match next_params {
                        Some(next_params) => Err(OdbcError::SendFailedAfterCheckpoint {
                            source,
                            next_params,
                            dropped_events: dropped_events
                                + pending_batches
                                    .as_slice()
                                    .iter()
                                    .map(Vec::len)
                                    .sum::<usize>(),
                        }),
                        None => Err(OdbcError::SendError { source }),
                    };
                }
            }
        }

        Ok(next_params)
    }

    fn enrich_events(&self, events: &mut [Event]) {
        let now = Utc::now();

        for event in events {
            let Event::Log(log) = event else {
                continue;
            };

            self.log_namespace
                .insert_standard_vector_source_metadata(log, OdbcConfig::NAME, now);
        }
    }
}

/// Closes the batch channel, waits for the blocking ODBC task, then returns
/// `OdbcError::Shutdown`. Received metrics for converted batches were already emitted
/// at conversion time.
async fn shutdown_query(
    rx: tokio::sync::mpsc::Receiver<Rows>,
    blocking: tokio::task::JoinHandle<Result<(), OdbcError>>,
) -> Result<Option<Vec<OdbcStatementParam>>, OdbcError> {
    drop(rx);
    let join_result = blocking.await.context(BlockingTaskSnafu);

    // Join errors are fatal. The query outcome is ignored on shutdown because the poll is
    // ending; converted rows were already counted at creation time.
    match join_result {
        Ok(_) => Err(OdbcError::Shutdown),
        Err(error) => Err(error),
    }
}

/// Sends an enriched batch to the pipeline in source-sender-sized chunks, racing against
/// shutdown between chunks so a large batch can stop promptly.
///
/// On `SendError::Closed`, emits `StreamClosedError` for the failed chunk so
/// `ComponentEventsDropped` is recorded (SourceSender discards its unsent count in that
/// case and expects the callee to emit). A timeout is reported by SourceSender as timed out, but
/// must also be reported as dropped when a tracking checkpoint makes retry impossible. Therefore,
/// the returned count excludes a closed failed chunk but includes a timed-out failed chunk.
///
/// On shutdown while `send_batch` is in flight, cancelling that future already makes
/// `UnsentEventCount` emit `ComponentEventsDropped` for the current chunk, so the returned
/// shutdown count excludes that chunk and only covers events not yet handed to SourceSender.
enum BatchSendError {
    Shutdown {
        dropped_events: usize,
    },
    Send {
        source: SendError,
        dropped_events: usize,
    },
}

async fn send_enriched_batch(
    out: &crate::SourceSender,
    events: Vec<Event>,
    shutdown: &mut ShutdownSignal,
) -> Result<(), BatchSendError> {
    let mut events = events.into_iter();
    let mut unsent_events = events.len();
    loop {
        let events: Vec<_> = events.by_ref().take(chunk_size_events()).collect();
        if events.is_empty() {
            break;
        }
        let count = events.len();
        let mut out = out.clone();
        let send_result = select! {
            _ = &mut *shutdown => {
                // SourceSender already emits ComponentEventsDropped for this chunk via
                // UnsentEventCount::drop when the cancelled send_batch future is dropped.
                return Err(BatchSendError::Shutdown {
                    dropped_events: unsent_events - count,
                });
            }
            send_result = out.send_batch(events) => send_result,
        };
        match send_result {
            Ok(()) => unsent_events -= count,
            Err(SendError::Closed) => {
                emit!(StreamClosedError { count });
                return Err(BatchSendError::Send {
                    source: SendError::Closed,
                    dropped_events: unsent_events - count,
                });
            }
            Err(SendError::Timeout) => {
                return Err(BatchSendError::Send {
                    source: SendError::Timeout,
                    dropped_events: unsent_events,
                });
            }
        }
    }
    Ok(())
}

/// Counts events in tracking batches that were received but not yet emitted.
fn discarded_tracking_event_count(pending_batches: &[Vec<Event>]) -> usize {
    pending_batches.iter().map(Vec::len).sum()
}

/// Emits `ComponentEventsDropped` for tracking batches that were counted as received
/// but discarded before pipeline emit (late query failure or conversion error).
fn emit_discarded_tracking_batches(pending_batches: &[Vec<Event>], reason: &'static str) {
    let count = discarded_tracking_event_count(pending_batches);
    if count > 0 {
        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count, reason });
    }
}

/// Records `BytesReceived` / `EventsReceived` for a converted batch before enrichment.
fn record_received(
    bytes_received: &Registered<BytesReceived>,
    events_received: &Registered<EventsReceived>,
    event_count: usize,
    byte_size: JsonSize,
) {
    if event_count == 0 {
        return;
    }
    bytes_received.emit(ByteSize(byte_size.get()));
    events_received.emit(CountByteSize(event_count, byte_size));
}

/// Converts ODBC result rows into log events without a JSON round-trip so typed
/// values such as timestamps and integers are preserved for downstream transforms.
fn rows_to_events(rows: Rows) -> Result<Vec<Event>, OdbcError> {
    let mut events = Vec::with_capacity(rows.len());

    for row in rows {
        let Value::Object(obj) = row else {
            return Err(OdbcError::InvalidResultRow);
        };

        events.push(LogEvent::from(obj).into());
    }

    Ok(events)
}

/// Extracts declared tracking columns from the final result row as SQL bind text.
///
/// Checkpoint values are stored as the exact SQL parameter text used for ODBC binding
/// so JSON roundtrips do not lose timestamp timezone formatting.
fn extract_tracking(
    obj: Value,
    tracking_columns: &[String],
    tz: Tz,
) -> Result<ObjectMap, OdbcError> {
    let Value::Object(obj) = obj else {
        return Err(OdbcError::InvalidTrackingRow);
    };

    let mut save_obj = ObjectMap::new();
    for column in tracking_columns {
        let (_, param) = resolve_tracking_column_parameter(&obj, column.as_str(), tz)?;
        save_obj.insert(
            KeyString::from(column.as_str()),
            Value::Bytes(Bytes::from(param)),
        );
    }
    Ok(save_obj)
}

/// Validates the final-row checkpoint, builds the next in-memory parameter list, then
/// persists tracking state. Persistence runs only after overlay succeeds so a bind-list
/// failure cannot advance an on-disk checkpoint that would skip unsent rows.
fn prepare_tracking_checkpoint(
    path: Option<&str>,
    last_row: Value,
    template: &[OdbcStatementParam],
    tracking_columns: &[String],
    tz: Tz,
) -> Result<Vec<OdbcStatementParam>, OdbcError> {
    let tracking = extract_tracking(last_row, tracking_columns, tz)?;
    let next_params = overlay_params(template, &tracking, tracking_columns, tz)?;
    if let Some(path) = path {
        save_params(path, &tracking)?;
    }
    Ok(next_params)
}

/// Returns an error when the query result contains duplicate column labels.
fn ensure_unique_column_names(names: &[String]) -> Result<(), OdbcError> {
    let mut seen = HashSet::with_capacity(names.len());
    let mut duplicates = Vec::new();

    for name in names {
        if !seen.insert(name.as_str()) {
            duplicates.push(name.clone());
        }
    }

    if duplicates.is_empty() {
        Ok(())
    } else {
        duplicates.sort_unstable();
        duplicates.dedup();
        Err(OdbcError::DuplicateColumnNames {
            columns: duplicates,
        })
    }
}

/// Returns true for ODBC binary column types that must be fetched with a binary buffer.
const fn is_binary_data_type(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Varbinary { .. } | DataType::Binary { .. } | DataType::LongVarbinary { .. }
    )
}

/// Caps a driver-reported cell size the same way `TextRowSet::for_cursor` does.
///
/// When `max_str_limit` is set, missing reports fall back to that upper bound.
/// When unset, a missing report cannot allocate a buffer.
fn capped_buffer_length(
    reported: Option<usize>,
    max_str_limit: Option<usize>,
    buffer_index: u16,
    batch_size: usize,
) -> Result<usize, OdbcError> {
    match max_str_limit {
        Some(upper_bound) => Ok(reported.unwrap_or(upper_bound).min(upper_bound)),
        None => reported.ok_or(OdbcError::Db {
            source: odbc_api::Error::TooLargeColumnBufferSize {
                buffer_index,
                num_elements: batch_size,
                element_size: usize::MAX,
            },
        }),
    }
}

/// Chooses a fetch buffer for one column.
///
/// Binary SQL types use `BufferDesc::Binary` with the octet length (not the hex
/// display size). All other types stay text so existing timestamp/decimal/tracking
/// text round-trips are unchanged.
///
/// `reported_fallback` is used when the SQL type does not carry a length: binary
/// columns fall back to `col_octet_length`, text columns to `col_display_size`.
fn buffer_desc_for_data_type(
    data_type: &DataType,
    reported_fallback: Option<usize>,
    max_str_limit: Option<usize>,
    buffer_index: u16,
    batch_size: usize,
) -> Result<BufferDesc, OdbcError> {
    if is_binary_data_type(data_type) {
        let reported = match data_type {
            DataType::Varbinary { length }
            | DataType::Binary { length }
            | DataType::LongVarbinary { length } => {
                length.map(NonZeroUsize::get).or(reported_fallback)
            }
            _ => reported_fallback,
        };
        let length = capped_buffer_length(reported, max_str_limit, buffer_index, batch_size)?;
        Ok(BufferDesc::Binary { length })
    } else {
        // Match `TextRowSet::for_cursor` / `utf8_display_sizes`: prefer UTF-8 length
        // from the SQL type, otherwise use the driver-reported fallback.
        let reported = data_type
            .utf8_len()
            .map(NonZeroUsize::get)
            .or(reported_fallback);
        let max_str_len = capped_buffer_length(reported, max_str_limit, buffer_index, batch_size)?;
        Ok(BufferDesc::Text { max_str_len })
    }
}

/// Builds per-column fetch buffers for a result set cursor.
fn buffer_descs_for_columns(
    cursor: &mut impl ResultSetMetadata,
    columns: &[Column],
    max_str_limit: Option<usize>,
    batch_size: usize,
) -> Result<Vec<BufferDesc>, OdbcError> {
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let col_index = (index + 1) as u16;
            let buffer_index = index as u16;

            let reported_fallback = if is_binary_data_type(&column.column_type) {
                match &column.column_type {
                    DataType::Varbinary { length: None }
                    | DataType::Binary { length: None }
                    | DataType::LongVarbinary { length: None } => cursor
                        .col_octet_length(col_index)
                        .context(DbSnafu)?
                        .map(NonZeroUsize::get),
                    _ => None,
                }
            } else if column.column_type.utf8_len().is_none() {
                cursor
                    .col_display_size(col_index)
                    .context(DbSnafu)?
                    .map(NonZeroUsize::get)
            } else {
                None
            };

            buffer_desc_for_data_type(
                &column.column_type,
                reported_fallback,
                max_str_limit,
                buffer_index,
                batch_size,
            )
        })
        .collect()
}

/// Returns whether a column needs the row-by-row `SQLGetData` path when fetch buffers are capped.
///
/// A row-set buffer cannot be enlarged after a forward-only cursor has fetched a truncated row.
/// Variable-width values therefore use `CursorRow::{get_text,get_binary}`, which grow their
/// destination buffer until the complete value has been read.
fn requires_streaming_fetch(data_type: &DataType, max_str_limit: usize) -> bool {
    match data_type {
        DataType::Char { .. }
        | DataType::WChar { .. }
        | DataType::Varchar { .. }
        | DataType::WVarchar { .. } => data_type
            .utf8_len()
            .map(NonZeroUsize::get)
            .is_none_or(|length| length > max_str_limit),
        DataType::Varbinary { length } | DataType::Binary { length } => length
            .map(NonZeroUsize::get)
            .is_none_or(|length| length > max_str_limit),
        // Long data types may have an imprecise driver-reported size, so never bind them to a
        // capped row-set buffer.
        DataType::LongVarchar { .. }
        | DataType::WLongVarchar { .. }
        | DataType::LongVarbinary { .. }
        | DataType::Unknown
        | DataType::Other { .. } => true,
        _ => false,
    }
}

/// Reads one cell through `SQLGetData`, growing the vector until the full cell is available.
fn read_streamed_cell(
    row: &mut CursorRow<'_>,
    column_index: u16,
    data_type: &DataType,
    initial_capacity: usize,
) -> Result<Option<Vec<u8>>, OdbcError> {
    let mut value = Vec::with_capacity(initial_capacity);
    let is_not_null = if is_binary_data_type(data_type) {
        row.get_binary(column_index, &mut value).context(DbSnafu)?
    } else {
        row.get_text(column_index, &mut value).context(DbSnafu)?
    };
    Ok(is_not_null.then_some(value))
}

/// Executes a result set one row at a time when any bound row-set buffer could truncate a cell.
fn execute_streaming_query<F>(
    mut cursor: impl Cursor,
    columns: &Columns,
    tz: Tz,
    batch_size: usize,
    initial_cell_capacity: usize,
    mut on_batch: F,
) -> Result<(), OdbcError>
where
    F: FnMut(Rows) -> Result<bool, OdbcError>,
{
    let mut batch_rows = Rows::with_capacity(batch_size);

    while let Some(mut row) = cursor.next_row().context(DbSnafu)? {
        let mut cols = ObjectMap::new();
        for (index, column) in columns.iter().enumerate() {
            let value = read_streamed_cell(
                &mut row,
                (index + 1) as u16,
                &column.column_type,
                initial_cell_capacity,
            )?;
            cols.insert(
                KeyString::from(column.column_name.as_str()),
                map_value(&column.column_type, value.as_deref(), tz),
            );
        }
        batch_rows.push(Value::Object(cols));

        if batch_rows.len() == batch_size {
            if !on_batch(mem::take(&mut batch_rows))? {
                return Ok(());
            }
            batch_rows.reserve(batch_size);
        }
    }

    if !batch_rows.is_empty() {
        on_batch(batch_rows)?;
    }
    Ok(())
}

/// Reads one cell from a columnar batch as optional bytes.
///
/// Only text and binary column buffers are allocated by [`buffer_descs_for_columns`].
fn cell_bytes<'a>(column: AnySlice<'a>, row_index: usize) -> Option<&'a [u8]> {
    if let Some(view) = column.as_bin_view() {
        view.get(row_index)
    } else if let Some(view) = column.as_text_view() {
        view.get(row_index)
    } else {
        unreachable!("ODBC fetch buffers are only text or binary")
    }
}

/// Executes an ODBC SQL query with optional parameters and invokes `on_batch` for each
/// fetched batch instead of accumulating the full result set in memory.
///
/// The callback returns `Ok(true)` to continue fetching or `Ok(false)` to stop early.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_query<F>(
    env: &Environment,
    conn_str: &str,
    stmt_str: &str,
    stmt_params: Vec<VarCharBox>,
    login_timeout: Duration,
    statement_timeout: Duration,
    tz: Tz,
    batch_size: usize,
    max_str_limit: Option<usize>,
    mut on_batch: F,
) -> Result<(), OdbcError>
where
    F: FnMut(Rows) -> Result<bool, OdbcError>,
{
    let conn_options = ConnectionOptions {
        login_timeout_sec: Some(login_timeout.as_secs() as u32),
        packet_size: None,
    };
    let conn = env
        .connect_with_connection_string(conn_str, conn_options)
        .context(DbSnafu)?;
    let mut statement = conn.preallocate().context(DbSnafu)?;
    statement
        .set_query_timeout_sec(statement_timeout.as_secs() as usize)
        .context(DbSnafu)?;

    let result = if stmt_params.is_empty() {
        statement.execute(stmt_str, ())
    } else {
        statement.execute(stmt_str, &stmt_params[..])
    }
    .context(DbSnafu)?;

    let Some(mut cursor) = result else {
        return Ok(());
    };

    let names = cursor
        .column_names()
        .context(DbSnafu)?
        .collect::<Result<Vec<String>, _>>()
        .context(DbSnafu)?;

    ensure_unique_column_names(&names)?;

    let types = (1..=names.len())
        .map(|col_index| cursor.col_data_type(col_index as u16).context(DbSnafu))
        .collect::<Result<Vec<_>, _>>()?;
    let columns = names
        .into_iter()
        .zip(types)
        .map(|(column_name, column_type)| Column {
            column_name,
            column_type,
        })
        .collect::<Columns>();

    // A capped row-set buffer is safe only if no result column can outgrow it. For
    // variable-width/unknown columns, read cells through SQLGetData instead of accepting a
    // truncation error after the forward-only cursor has already advanced.
    if let Some(limit) = max_str_limit
        && columns
            .iter()
            .any(|column| requires_streaming_fetch(&column.column_type, limit))
    {
        return execute_streaming_query(cursor, &columns, tz, batch_size, limit.min(256), on_batch);
    }

    let descs = buffer_descs_for_columns(&mut cursor, &columns, max_str_limit, batch_size)?;
    let buffer = ColumnarAnyBuffer::try_from_descs(batch_size, descs).context(DbSnafu)?;
    let mut row_set_cursor = cursor.bind_buffer(buffer).context(DbSnafu)?;
    let mut batch_rows = Rows::with_capacity(batch_size);

    while let Some(batch) = row_set_cursor
        .fetch_with_truncation_check(true)
        .context(DbSnafu)?
    {
        let num_rows = batch.num_rows();

        for row_index in 0..num_rows {
            let mut cols = ObjectMap::new();

            for (index, column) in columns.iter().enumerate() {
                let data_name = &column.column_name;
                let data_type = &column.column_type;
                let data_value = cell_bytes(batch.column(index), row_index);
                let key = KeyString::from(data_name.as_str());
                let value = map_value(data_type, data_value, tz);
                cols.insert(key, value);
            }

            batch_rows.push(Value::Object(cols));
        }

        if !batch_rows.is_empty() {
            if !on_batch(mem::take(&mut batch_rows))? {
                break;
            }
            batch_rows.reserve(batch_size);
        }
    }

    Ok(())
}

/// Loads tracked column overlays from disk.
///
/// Returns `Ok(None)` only when the metadata file does not exist.
fn load_tracking_map(path: &str) -> Result<Option<ObjectMap>, OdbcError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(OdbcError::Io { source }),
    };
    let reader = BufReader::new(file);
    let map: ObjectMap = serde_json::from_reader(reader).context(JsonSnafu)?;
    Ok(Some(map))
}

/// Resolves a single statement parameter value.
///
/// Tracking columns use `overlay` when present; otherwise the configured value is kept.
fn resolve_param_value(
    param: &OdbcStatementParam,
    overlay: Option<&ObjectMap>,
    tracking: &HashSet<&str>,
    tz: Tz,
) -> Result<String, OdbcError> {
    match (tracking.contains(param.name.as_str()), overlay) {
        (true, Some(overlay)) => {
            Ok(resolve_tracking_column_parameter(overlay, param.name.as_str(), tz)?.1)
        }
        _ => Ok(param.value.clone()),
    }
}

/// Builds ODBC bind parameters from the ordered `statement_init_params` list.
///
/// Array order is the bind order. When `overlay` is set, tracking-column values are
/// taken from it; non-tracking values always keep the template entry.
fn order_params(
    params: &[OdbcStatementParam],
    overlay: Option<&ObjectMap>,
    tracking_columns: Option<&[String]>,
    tz: Tz,
) -> Result<Vec<VarCharBox>, OdbcError> {
    let tracking: HashSet<&str> = tracking_columns
        .unwrap_or_default()
        .iter()
        .map(String::as_str)
        .collect();

    params
        .iter()
        .map(|param| {
            resolve_param_value(param, overlay, &tracking, tz).map(|value| value.into_parameter())
        })
        .collect()
}

/// Returns `params` with tracking-column values overlaid from `overlay`.
///
/// Non-tracking entries and overall array order are preserved.
fn overlay_params(
    params: &[OdbcStatementParam],
    overlay: &ObjectMap,
    tracking_columns: &[String],
    tz: Tz,
) -> Result<Vec<OdbcStatementParam>, OdbcError> {
    let tracking: HashSet<&str> = tracking_columns.iter().map(String::as_str).collect();

    params
        .iter()
        .map(|param| {
            Ok(OdbcStatementParam {
                name: param.name.clone(),
                value: resolve_param_value(param, Some(overlay), &tracking, tz)?,
            })
        })
        .collect()
}

/// Creates parent directories for the metadata path when needed.
pub(crate) fn prepare_metadata_path(path: &str) -> Result<(), OdbcError> {
    if path.is_empty() {
        return Err(OdbcError::Io {
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "`last_run_metadata_path` must not be empty",
            ),
        });
    }

    let path = Path::new(path);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    fs::create_dir_all(parent).context(IoSnafu)
}

/// Returns a sibling temp path that is always distinct from `path`.
///
/// Appending `.tmp` to the full filename avoids `Path::with_extension("tmp")`
/// returning the destination path when it already ends in `.tmp`.
fn checkpoint_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| format!("{}.tmp", name.to_string_lossy()))
        .unwrap_or_else(|| "checkpoint.tmp".into());
    path.with_file_name(file_name)
}

/// Replaces `path` with the contents of `tmp_path` atomically.
///
/// On Windows, `rename` cannot replace an existing destination file, so use
/// `ReplaceFileW` and fall back to `rename` when the destination does not exist yet.
fn replace_checkpoint_file(tmp_path: &Path, path: &Path) -> Result<(), OdbcError> {
    #[cfg(windows)]
    {
        use windows::Win32::Storage::FileSystem::ReplaceFileW;
        use windows::core::HSTRING;

        let dst = HSTRING::from(path.to_string_lossy().as_ref());
        let src = HSTRING::from(tmp_path.to_string_lossy().as_ref());
        let replaced = unsafe {
            ReplaceFileW(
                &dst,
                &src,
                None,
                windows::Win32::Storage::FileSystem::REPLACE_FILE_FLAGS(0),
                None,
                None,
            )
        };
        if replaced.is_ok() {
            return Ok(());
        }
        // Destination may not exist yet — fall back to rename.
    }

    fs::rename(tmp_path, path).context(IoSnafu)
}

/// Serializes and persists the latest tracked values for reuse as SQL parameters.
///
/// Writes to a sibling `.tmp` file and atomically replaces the checkpoint on success.
fn save_params(path: &str, obj: &ObjectMap) -> Result<(), OdbcError> {
    prepare_metadata_path(path)?;
    let path = Path::new(path);
    let tmp_path = checkpoint_temp_path(path);
    let json = serde_json::to_string(obj).context(JsonSnafu)?;

    {
        let mut file = fs::File::create(&tmp_path).context(IoSnafu)?;
        file.write_all(json.as_bytes()).context(IoSnafu)?;
        file.sync_all().context(IoSnafu)?;
    }

    replace_checkpoint_file(&tmp_path, path)
}

/// Localizes a naive datetime with `tz`, using `.latest()` for DST ambiguity and
/// preserving `fallback_text` when the local time does not exist.
fn naive_local_to_timestamp_value(ndt: NaiveDateTime, tz: Tz, fallback_text: &str) -> Value {
    if let Some(dt) = ndt.and_local_timezone(tz).latest() {
        Value::Timestamp(dt.with_timezone(&Utc))
    } else {
        Value::Bytes(Bytes::copy_from_slice(fallback_text.as_bytes()))
    }
}

/// Returns true for SQL-style timestamps with a space date/time separator and offset suffix,
/// such as `YYYY-MM-DD HH:MM:SS+00` or `YYYY-MM-DD HH:MM:SS-05:00`. Returns false for
/// RFC3339 forms that use a `T` separator or a `Z` suffix; those are detected with
/// `DateTime::parse_from_rfc3339` instead.
fn sql_timestamp_text_has_offset(text: &str) -> bool {
    if text.len() <= 10 {
        return false;
    }

    // SQL form uses a space between date and time; RFC3339 uses `T`.
    if text.as_bytes().get(10) != Some(&b' ') {
        return false;
    }

    let Some(tail) = text.get(10..) else {
        return false;
    };
    let Some(sign_idx) = tail.rfind(['+', '-']) else {
        return false;
    };

    let Some(offset) = text.get(10 + sign_idx..) else {
        return false;
    };
    offset.len() >= 2
        && offset
            .get(1..)
            .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit() || c == ':'))
}

/// Returns true when timestamp text carries an explicit zone that must be preserved
/// for tracking parameter round-trips.
///
/// Covers SQL-style offsets (`YYYY-MM-DD HH:MM:SS+02:00`) and any valid RFC3339 value
/// (which always includes an offset or `Z`). Converting those to `Value::Timestamp`
/// would drop the original offset and later rebind a naive local datetime in
/// `odbc_default_timezone`.
fn timestamp_text_has_preserved_offset(text: &str) -> bool {
    sql_timestamp_text_has_offset(text) || DateTime::parse_from_rfc3339(text).is_ok()
}

/// Maps ODBC timestamp bytes to a Vector value.
///
/// Offset-bearing SQL and RFC3339 forms are preserved as bytes so tracking parameters
/// round-trip the exact ODBC text. Naive timestamps are parsed to `Value::Timestamp`
/// using `tz`.
fn map_timestamp_value(value: &[u8], tz: Tz) -> Value {
    let Ok(text) = std::str::from_utf8(value) else {
        return Value::Bytes(Bytes::copy_from_slice(value));
    };

    if timestamp_text_has_preserved_offset(text) {
        return Value::Bytes(Bytes::copy_from_slice(value));
    }

    TIMESTAMP_FORMATS
        .iter()
        .find_map(|fmt| NaiveDateTime::parse_from_str(text, fmt).ok())
        .map(|ndt| naive_local_to_timestamp_value(ndt, tz, text))
        .unwrap_or_else(|| Value::Bytes(Bytes::copy_from_slice(value)))
}

/// Converts ODBC data types to Vector values.
///
/// # Arguments
/// * `data_type`: The ODBC data type.
/// * `value`: The ODBC value to convert. Binary columns are raw bytes from a binary
///   buffer; character and other text-fetched columns are driver text bytes.
/// * `tz`: The timezone to use for date/time conversions.
///
/// # Returns
/// A `Value` compatible with Vector events.
fn map_value(data_type: &DataType, value: Option<&[u8]>, tz: Tz) -> Value {
    match data_type {
        // Character / unknown text-fetched columns.
        DataType::Unknown
        | DataType::Char { .. }
        | DataType::WChar { .. }
        | DataType::Varchar { .. }
        | DataType::WVarchar { .. }
        | DataType::LongVarchar { .. }
        | DataType::WLongVarchar { .. }
        | DataType::Other { .. } => {
            let Some(value) = value else {
                return Value::Null;
            };

            Value::Bytes(Bytes::copy_from_slice(value))
        }

        // Binary columns are fetched with `BufferDesc::Binary` so these bytes are the
        // original octet sequence, not ODBC's hex text conversion of binary values.
        DataType::Varbinary { .. } | DataType::Binary { .. } | DataType::LongVarbinary { .. } => {
            let Some(value) = value else {
                return Value::Null;
            };

            Value::Bytes(Bytes::copy_from_slice(value))
        }

        // Convert to integer.
        DataType::TinyInt | DataType::SmallInt | DataType::BigInt | DataType::Integer => {
            let Some(value) = value else {
                return Value::Null;
            };

            // Preserve unrepresentable integers as bytes so tracking metadata is not lost.
            match std::str::from_utf8(value).map(|s| s.parse::<i64>()) {
                Ok(Ok(i)) => Value::Integer(i),
                _ => Value::Bytes(Bytes::copy_from_slice(value)),
            }
        }

        // Convert to float.
        DataType::Float { .. } | DataType::Real | DataType::Double => {
            let Some(value) = value else {
                return Value::Null;
            };

            // Preserve unrepresentable floats (for example NaN) as bytes so tracking metadata is not lost.
            // Downstream consumers may see `Value::Bytes` instead of `Value::Float` for NaN and other
            // values that `NotNan` cannot represent.
            match std::str::from_utf8(value).map(NotNan::from_str) {
                Ok(Ok(f)) => Value::Float(f),
                _ => Value::Bytes(Bytes::copy_from_slice(value)),
            }
        }

        // Preserve exact decimal values from the database.
        DataType::Decimal { .. } | DataType::Numeric { .. } => {
            let Some(value) = value else {
                return Value::Null;
            };

            Value::Bytes(Bytes::copy_from_slice(value))
        }

        // Convert to timestamp.
        DataType::Timestamp { .. } => {
            let Some(value) = value else {
                return Value::Null;
            };

            map_timestamp_value(value, tz)
        }

        // Preserve the original time text so tracking parameters bind as `HH:MM:SS`
        // instead of a full timestamp such as `1970-01-01 15:30:00`.
        // MariaDB TIME can represent durations outside a clock-of-day range (for example
        // `25:00:00`), so keep the ODBC text even when chrono cannot parse it.
        DataType::Time { .. } => {
            let Some(value) = value else {
                return Value::Null;
            };

            Value::Bytes(Bytes::copy_from_slice(value))
        }

        // Preserve the original date text so tracking parameters bind as `YYYY-MM-DD`
        // instead of a full timestamp such as `2025-10-04 00:00:00`.
        // MariaDB/MySQL zero dates such as `0000-00-00` are not chrono-compatible but
        // remain valid for SQL comparison and tracking parameter binding.
        DataType::Date => {
            let Some(value) = value else {
                return Value::Null;
            };

            Value::Bytes(Bytes::copy_from_slice(value))
        }

        // Convert to boolean.
        // Some ODBC drivers return a non-NULL BIT with an empty buffer; treat that as null
        // instead of panicking on an empty slice.
        DataType::Bit => {
            let Some(value) = value else {
                return Value::Null;
            };

            match value.first().copied() {
                Some(b) => Value::Boolean(b == 1 || b == b'1'),
                None => Value::Null,
            }
        }
    }
}

/// Formats a UTC timestamp as a naive local datetime string for ODBC parameter binding.
fn format_timestamp_for_sql_parameter(timestamp: DateTime<Utc>, tz: Tz) -> String {
    let local = timestamp.with_timezone(&tz);
    if local.nanosecond() != 0 {
        local.format("%Y-%m-%d %H:%M:%S%.f").to_string()
    } else {
        local.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

/// Validates that `map` contains every declared tracking column with a value that can
/// be converted to an ODBC parameter.
pub(crate) fn validate_tracking_state(
    map: &ObjectMap,
    tracking_columns: &[String],
    tz: Tz,
) -> Result<(), String> {
    for column in tracking_columns {
        resolve_tracking_column_parameter(map, column.as_str(), tz)
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

/// Resolves a single tracking column to its source value and the text used for ODBC
/// parameter binding.
fn resolve_tracking_column_parameter(
    map: &ObjectMap,
    column: &str,
    tz: Tz,
) -> Result<(Value, String), OdbcError> {
    let value = map
        .get(column)
        .ok_or_else(|| OdbcError::MissingTrackingColumn {
            column: column.to_owned(),
        })?;
    let param =
        value_to_sql_parameter(value, tz).ok_or_else(|| OdbcError::InvalidTrackingValue {
            column: column.to_owned(),
        })?;
    Ok((value.clone(), param))
}

/// Converts a scalar VRL value to raw text for ODBC parameter binding.
///
/// Unlike `Value::to_string()`, this does not use VRL literal syntax (e.g. quoted
/// strings or `t'…'` timestamps).
///
/// Only `Value::Timestamp` is reformatted in `tz` as a naive local datetime.
/// Byte/string values are preserved as-is so VARCHAR tracking columns that
/// happen to look like RFC3339 are not coerced into timestamp predicates.
/// Non-UTF-8 bytes (for example raw `VARBINARY` payloads) cannot be bound as
/// text parameters and return `None`.
fn value_to_sql_parameter(value: &Value, tz: Tz) -> Option<String> {
    match value {
        Value::Integer(i) => Some(i.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Boolean(b) => Some(boolean_to_sql_parameter(*b)),
        Value::Bytes(b) => std::str::from_utf8(b).ok().map(str::to_owned),
        Value::Timestamp(t) => Some(format_timestamp_for_sql_parameter(*t, tz)),
        Value::Null => None,
        other => serde_json::to_value(other).ok().and_then(|v| match v {
            serde_json::Value::String(s) => Some(s),
            serde_json::Value::Number(n) => Some(n.to_string()),
            serde_json::Value::Bool(b) => Some(boolean_to_sql_parameter(b)),
            _ => None,
        }),
    }
}

/// Formats a boolean as `1`/`0` for ODBC parameter binding.
///
/// Numeric/bit columns (for example MariaDB `BIT`) coerce string parameters to
/// numbers; `"true"` and `"false"` both become `0`, so use bit literals instead.
fn boolean_to_sql_parameter(value: bool) -> String {
    if value { "1" } else { "0" }.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use chrono::TimeZone;
    use vrl::event_path;

    #[test]
    fn rows_to_events_preserves_typed_values() {
        let timestamp = chrono::Utc
            .with_ymd_and_hms(2025, 10, 4, 12, 34, 56)
            .unwrap();
        let mut row = ObjectMap::new();
        row.insert(KeyString::from("id"), Value::Integer(1));
        row.insert(KeyString::from("active"), Value::Boolean(true));
        row.insert(KeyString::from("datetime_col"), Value::Timestamp(timestamp));
        row.insert(
            KeyString::from("date_col"),
            Value::Bytes(Bytes::from_static(b"2025-10-04")),
        );

        let rows = vec![Value::Object(row)];
        let events = rows_to_events(rows).expect("events");

        assert_eq!(events.len(), 1);

        let Event::Log(log) = &events[0] else {
            panic!("expected log event");
        };

        assert_eq!(log.get(event_path!("id")).unwrap(), &Value::Integer(1));
        assert_eq!(
            log.get(event_path!("active")).unwrap(),
            &Value::Boolean(true)
        );
        assert_eq!(
            log.get(event_path!("datetime_col")).unwrap(),
            &Value::Timestamp(timestamp)
        );
        assert_eq!(
            log.get(event_path!("date_col")).unwrap(),
            &Value::Bytes(Bytes::from_static(b"2025-10-04"))
        );
    }

    #[test]
    fn rows_to_events_errors_on_non_object_row() {
        let rows = vec![Value::Integer(1)];
        let error = rows_to_events(rows).expect_err("expected error");
        assert!(matches!(error, OdbcError::InvalidResultRow));
    }

    #[test]
    fn discarded_tracking_event_count_sums_buffered_rows() {
        let batches = vec![
            vec![
                Event::Log(LogEvent::from("one")),
                Event::Log(LogEvent::from("two")),
            ],
            vec![Event::Log(LogEvent::from("three"))],
        ];

        assert_eq!(discarded_tracking_event_count(&batches), 3);
        assert_eq!(discarded_tracking_event_count(&[]), 0);
    }

    #[tokio::test]
    async fn send_enriched_batch_excludes_in_flight_chunk_from_shutdown_drops() {
        let (mut out, _recv) = crate::SourceSender::new_test_sender_with_options(1, None);
        out.send_batch(vec![Event::Log(LogEvent::from("already buffered"))])
            .await
            .expect("first batch should fill the output buffer");

        let (trigger_shutdown, mut shutdown, _) = ShutdownSignal::new_wired();
        drop(trigger_shutdown);

        // Both events fit in one SourceSender chunk. Cancelling the blocked send_batch
        // already drops that in-flight chunk via UnsentEventCount, so the shutdown error
        // must report 0 additional drops.
        let result = send_enriched_batch(
            &out,
            vec![
                Event::Log(LogEvent::from("unsent one")),
                Event::Log(LogEvent::from("unsent two")),
            ],
            &mut shutdown,
        )
        .await;

        assert!(matches!(
            result,
            Err(BatchSendError::Shutdown { dropped_events: 0 })
        ));
    }

    #[test]
    fn map_value_bit_binary_and_text() {
        assert_eq!(
            map_value(&odbc_api::DataType::Bit, Some(&[1]), chrono_tz::UTC),
            Value::Boolean(true)
        );
        assert_eq!(
            map_value(&odbc_api::DataType::Bit, Some(&[0]), chrono_tz::UTC),
            Value::Boolean(false)
        );
        assert_eq!(
            map_value(&odbc_api::DataType::Bit, Some(b"1"), chrono_tz::UTC),
            Value::Boolean(true)
        );
        assert_eq!(
            map_value(&odbc_api::DataType::Bit, Some(b"0"), chrono_tz::UTC),
            Value::Boolean(false)
        );
    }

    #[test]
    fn map_value_bit_empty_buffer_maps_to_null() {
        assert_eq!(
            map_value(&odbc_api::DataType::Bit, Some(&[]), chrono_tz::UTC),
            Value::Null
        );
    }

    #[test]
    fn map_value_integer_in_range() {
        let value = map_value(
            &odbc_api::DataType::BigInt,
            Some(b"9223372036854775807"),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Integer(9223372036854775807));
    }

    #[test]
    fn map_value_integer_out_of_range_preserved_as_bytes() {
        let raw = b"18446744073709551615";
        let value = map_value(&odbc_api::DataType::BigInt, Some(raw), chrono_tz::UTC);
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("18446744073709551615".to_owned())
        );
    }

    #[test]
    fn map_value_binary_preserves_raw_bytes_not_hex_text() {
        // ODBC text conversion would turn 0x00FF into ASCII "00FF"; binary fetch must keep
        // the original octets so event payloads and any future binary binding stay correct.
        let raw = &[0x00, 0xFF, 0x10];
        for data_type in [
            DataType::Varbinary {
                length: NonZeroUsize::new(3),
            },
            DataType::Binary {
                length: NonZeroUsize::new(3),
            },
            DataType::LongVarbinary {
                length: NonZeroUsize::new(3),
            },
        ] {
            let value = map_value(&data_type, Some(raw), chrono_tz::UTC);
            assert_eq!(value, Value::Bytes(Bytes::copy_from_slice(raw)));
            assert_eq!(
                value_to_sql_parameter(&value, chrono_tz::UTC),
                None,
                "non-UTF-8 binary payloads must not bind as text tracking parameters"
            );
        }
    }

    #[test]
    fn capped_buffer_length_matches_text_row_set_rules() {
        assert_eq!(
            capped_buffer_length(Some(8192), Some(4096), 0, 100).unwrap(),
            4096
        );
        assert_eq!(
            capped_buffer_length(None, Some(4096), 0, 100).unwrap(),
            4096
        );
        assert_eq!(
            capped_buffer_length(Some(128), Some(4096), 0, 100).unwrap(),
            128
        );
        assert!(matches!(
            capped_buffer_length(None, None, 2, 50),
            Err(OdbcError::Db {
                source: odbc_api::Error::TooLargeColumnBufferSize {
                    buffer_index: 2,
                    num_elements: 50,
                    element_size: usize::MAX,
                },
            })
        ));
    }

    #[test]
    fn binary_buffer_desc_uses_octet_length_not_hex_display_size() {
        // ODBC display_size for VARBINARY(3) is 6 (two hex chars per byte). Fetching with a
        // text buffer of that size is what caused the hex-text bug; binary buffers must use 3.
        assert_eq!(
            DataType::Varbinary {
                length: NonZeroUsize::new(3),
            }
            .display_size()
            .map(NonZeroUsize::get),
            Some(6)
        );

        let desc = buffer_desc_for_data_type(
            &DataType::Varbinary {
                length: NonZeroUsize::new(3),
            },
            None,
            Some(4096),
            0,
            100,
        )
        .unwrap();
        assert_eq!(desc, BufferDesc::Binary { length: 3 });

        let text_desc = buffer_desc_for_data_type(
            &DataType::Varchar {
                length: NonZeroUsize::new(3),
            },
            None,
            Some(4096),
            0,
            100,
        )
        .unwrap();
        // VARCHAR(3) UTF-8 buffer is 3 * 4 = 12, matching TextRowSet::for_cursor.
        assert_eq!(text_desc, BufferDesc::Text { max_str_len: 12 });
    }

    #[test]
    fn is_binary_data_type_detects_binary_sql_types() {
        assert!(is_binary_data_type(&DataType::Varbinary {
            length: NonZeroUsize::new(16)
        }));
        assert!(is_binary_data_type(&DataType::Binary {
            length: NonZeroUsize::new(16)
        }));
        assert!(is_binary_data_type(&DataType::LongVarbinary {
            length: None
        }));
        assert!(!is_binary_data_type(&DataType::Varchar {
            length: NonZeroUsize::new(16)
        }));
        assert!(!is_binary_data_type(&DataType::Unknown));
        assert!(!is_binary_data_type(&DataType::Integer));
    }

    #[test]
    fn variable_width_columns_use_streaming_fetch_with_a_buffer_limit() {
        assert!(requires_streaming_fetch(
            &DataType::LongVarchar { length: None },
            4096
        ));
        assert!(!requires_streaming_fetch(
            &DataType::Varbinary {
                length: NonZeroUsize::new(16),
            },
            4096
        ));
        assert!(requires_streaming_fetch(
            &DataType::Varbinary {
                length: NonZeroUsize::new(4097),
            },
            4096
        ));
        assert!(!requires_streaming_fetch(&DataType::Integer, 4096));
        assert!(!requires_streaming_fetch(
            &DataType::Char {
                length: NonZeroUsize::new(16),
            },
            4096
        ));
        assert!(requires_streaming_fetch(
            &DataType::Char {
                length: NonZeroUsize::new(2048),
            },
            4096
        ));
    }

    #[test]
    fn map_value_time_preserved_as_bytes_for_tracking_bind() {
        let raw = b"15:30:00";
        let value = map_value(
            &odbc_api::DataType::Time { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("15:30:00".to_owned())
        );
    }

    #[test]
    fn map_value_date_preserved_as_bytes_for_tracking_bind() {
        let raw = b"2025-10-04";
        let value = map_value(&odbc_api::DataType::Date, Some(raw), chrono_tz::UTC);
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("2025-10-04".to_owned())
        );
    }

    #[test]
    fn map_value_zero_date_preserved_as_bytes_for_tracking_bind() {
        let raw = b"0000-00-00";
        let value = map_value(&odbc_api::DataType::Date, Some(raw), chrono_tz::UTC);
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("0000-00-00".to_owned())
        );
    }

    #[test]
    fn map_value_duration_time_preserved_as_bytes_for_tracking_bind() {
        let raw = b"25:00:00";
        let value = map_value(
            &odbc_api::DataType::Time { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("25:00:00".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_with_offset_preserved_as_bytes_for_tracking_bind() {
        let raw = b"2025-04-28 01:20:04+00";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("2025-04-28 01:20:04+00".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_with_offset_and_colon_preserved_as_bytes_for_tracking_bind() {
        let raw = b"2025-04-28 01:20:04+00:00";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("2025-04-28 01:20:04+00:00".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_with_negative_offset_preserved_as_bytes_for_tracking_bind() {
        let raw = b"2025-04-28 01:20:04-05:00";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("2025-04-28 01:20:04-05:00".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_rfc3339_z_preserved_as_bytes_for_tracking_bind() {
        let raw = b"2025-04-28T01:20:04Z";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("2025-04-28T01:20:04Z".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_rfc3339_t_offset_preserved_as_bytes_for_tracking_bind() {
        let raw = b"2025-04-28T01:20:04+00:00";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("2025-04-28T01:20:04+00:00".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_rfc3339_non_utc_offset_preserved_as_bytes_for_tracking_bind() {
        // Rebinding through Value::Timestamp would yield a naive local time in
        // odbc_default_timezone and can skip/replay rows when the DB offset differs.
        let raw = b"2025-04-28T01:20:04+02:00";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::Asia::Seoul,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::Asia::Seoul),
            Some("2025-04-28T01:20:04+02:00".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_rfc3339_fractional_offset_preserved_as_bytes_for_tracking_bind() {
        let raw = b"2025-04-28T01:20:04.123456+02:00";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 6 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("2025-04-28T01:20:04.123456+02:00".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_unparseable_preserved_as_bytes() {
        let raw = b"not-a-timestamp";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(value, Value::Bytes(Bytes::from_static(raw)));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::UTC),
            Some("not-a-timestamp".to_owned())
        );
    }

    #[test]
    fn map_value_timestamp_naive_parsed_to_timestamp() {
        let raw = b"2025-10-04 12:34:56";
        let value = map_value(
            &odbc_api::DataType::Timestamp { precision: 0 },
            Some(raw),
            chrono_tz::UTC,
        );
        assert_eq!(
            value,
            Value::Timestamp(
                chrono::Utc
                    .with_ymd_and_hms(2025, 10, 4, 12, 34, 56)
                    .unwrap()
            )
        );
    }

    #[test]
    fn value_to_sql_parameter_preserves_rfc3339_looking_string_bytes() {
        // VARCHAR/TEXT tracking values must round-trip unchanged even when they
        // parse as RFC3339; only Value::Timestamp is reformatted.
        let raw = "2024-06-01T00:00:00Z";
        let value = Value::Bytes(Bytes::from_static(raw.as_bytes()));
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::Asia::Seoul),
            Some(raw.to_owned())
        );
    }

    #[test]
    fn value_to_sql_parameter_formats_timestamp_in_odbc_timezone() {
        let value = Value::Timestamp(chrono::Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap());
        assert_eq!(
            value_to_sql_parameter(&value, chrono_tz::Asia::Seoul),
            Some("2024-06-01 09:00:00".to_owned())
        );
    }

    #[test]
    fn order_params_preserves_static_and_array_order() {
        let params = vec![
            OdbcStatementParam {
                name: "tenant_id".to_owned(),
                value: "acme".to_owned(),
            },
            OdbcStatementParam {
                name: "id".to_owned(),
                value: "0".to_owned(),
            },
            OdbcStatementParam {
                name: "region".to_owned(),
                value: "us-east".to_owned(),
            },
        ];
        let mut overlay = ObjectMap::new();
        overlay.insert(KeyString::from("id"), Value::Integer(42));
        let tracking = vec!["id".to_owned()];

        let bound = order_params(&params, Some(&overlay), Some(&tracking), chrono_tz::UTC)
            .expect("order params");

        assert_eq!(bound.len(), 3);
        // Array order is the bind order even when static params surround tracking ones.
        let expected = ["acme", "42", "us-east"];
        for (param, expected) in bound.iter().zip(expected) {
            assert_eq!(
                std::str::from_utf8(param.as_bytes().expect("bound bytes")).expect("utf-8"),
                expected
            );
        }
    }

    #[test]
    fn order_params_errors_on_missing_tracking_overlay() {
        let params = vec![
            OdbcStatementParam {
                name: "tenant_id".to_owned(),
                value: "acme".to_owned(),
            },
            OdbcStatementParam {
                name: "id".to_owned(),
                value: "0".to_owned(),
            },
        ];
        let overlay = ObjectMap::new();
        let tracking = vec!["id".to_owned()];

        let error = match order_params(&params, Some(&overlay), Some(&tracking), chrono_tz::UTC) {
            Err(error) => error,
            Ok(_) => panic!("expected missing tracking column error"),
        };

        assert!(matches!(
            error,
            OdbcError::MissingTrackingColumn { column } if column == "id"
        ));
    }

    #[test]
    fn overlay_params_keeps_static_values() {
        let params = vec![
            OdbcStatementParam {
                name: "tenant_id".to_owned(),
                value: "acme".to_owned(),
            },
            OdbcStatementParam {
                name: "id".to_owned(),
                value: "0".to_owned(),
            },
        ];
        let mut overlay = ObjectMap::new();
        overlay.insert(
            KeyString::from("id"),
            Value::Bytes(Bytes::from_static(b"42")),
        );
        let tracking = vec!["id".to_owned()];

        let next = overlay_params(&params, &overlay, &tracking, chrono_tz::UTC).expect("overlay");

        assert_eq!(
            next,
            vec![
                OdbcStatementParam {
                    name: "tenant_id".to_owned(),
                    value: "acme".to_owned(),
                },
                OdbcStatementParam {
                    name: "id".to_owned(),
                    value: "42".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn extract_tracking_errors_on_missing_column() {
        let mut obj = ObjectMap::new();
        obj.insert(KeyString::from("id"), Value::Integer(1));

        let error = extract_tracking(
            Value::Object(obj),
            &["id".to_owned(), "name".to_owned()],
            chrono_tz::UTC,
        )
        .expect_err("expected missing tracking column error");

        assert!(matches!(
            error,
            OdbcError::MissingTrackingColumn { column } if column == "name"
        ));
    }

    #[test]
    fn extract_tracking_errors_on_null_tracking_value() {
        let mut obj = ObjectMap::new();
        obj.insert(KeyString::from("id"), Value::Null);

        let error = extract_tracking(Value::Object(obj), &["id".to_owned()], chrono_tz::UTC)
            .expect_err("expected invalid tracking value error");

        assert!(matches!(
            error,
            OdbcError::InvalidTrackingValue { column } if column == "id"
        ));
    }

    #[test]
    fn extract_tracking_errors_on_invalid_tracking_row() {
        let error = extract_tracking(Value::Integer(1), &["id".to_owned()], chrono_tz::UTC)
            .expect_err("expected invalid tracking row error");

        assert!(matches!(error, OdbcError::InvalidTrackingRow));
    }

    #[test]
    fn prepare_tracking_checkpoint_does_not_persist_when_extract_fails() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let path = temp_dir.path().join("tracking.json");
        let path = path.to_str().expect("utf-8 path");
        let mut obj = ObjectMap::new();
        obj.insert(KeyString::from("id"), Value::Null);
        let template = vec![OdbcStatementParam {
            name: "id".to_owned(),
            value: "0".to_owned(),
        }];

        let error = prepare_tracking_checkpoint(
            Some(path),
            Value::Object(obj),
            &template,
            &["id".to_owned()],
            chrono_tz::UTC,
        )
        .expect_err("expected invalid tracking value");

        assert!(matches!(
            error,
            OdbcError::InvalidTrackingValue { column } if column == "id"
        ));
        assert!(
            !std::path::Path::new(path).exists(),
            "checkpoint must not be written when extract fails"
        );
    }

    #[test]
    fn prepare_tracking_checkpoint_persists_after_overlay() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let path = temp_dir.path().join("tracking.json");
        let path = path.to_str().expect("utf-8 path");
        let mut obj = ObjectMap::new();
        obj.insert(KeyString::from("id"), Value::Integer(42));
        obj.insert(KeyString::from("name"), Value::from("vector"));
        let template = vec![
            OdbcStatementParam {
                name: "tenant_id".to_owned(),
                value: "acme".to_owned(),
            },
            OdbcStatementParam {
                name: "id".to_owned(),
                value: "0".to_owned(),
            },
            OdbcStatementParam {
                name: "name".to_owned(),
                value: "init".to_owned(),
            },
        ];

        let next = prepare_tracking_checkpoint(
            Some(path),
            Value::Object(obj),
            &template,
            &["id".to_owned(), "name".to_owned()],
            chrono_tz::UTC,
        )
        .expect("prepared tracking checkpoint");

        assert_eq!(
            next,
            vec![
                OdbcStatementParam {
                    name: "tenant_id".to_owned(),
                    value: "acme".to_owned(),
                },
                OdbcStatementParam {
                    name: "id".to_owned(),
                    value: "42".to_owned(),
                },
                OdbcStatementParam {
                    name: "name".to_owned(),
                    value: "vector".to_owned(),
                },
            ]
        );

        let saved = load_tracking_map(path)
            .expect("load checkpoint")
            .expect("checkpoint exists");
        assert_eq!(
            saved.get("id"),
            Some(&Value::Bytes(Bytes::from_static(b"42")))
        );
        assert_eq!(
            saved.get("name"),
            Some(&Value::Bytes(Bytes::from_static(b"vector")))
        );
    }

    #[test]
    fn load_tracking_map_reads_valid_tracking_metadata() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let path = temp_dir.path().join("tracking.json");
        fs::write(&path, r#"{"id":1,"name":"vector"}"#).expect("write metadata");
        let path = path.to_str().expect("utf-8 path");

        let map = load_tracking_map(path)
            .expect("load tracking map")
            .expect("metadata map");

        assert_eq!(map.get("id"), Some(&Value::Integer(1)));
        assert_eq!(map.get("name"), Some(&Value::from("vector")));
    }

    #[test]
    fn validate_tracking_state_errors_on_missing_column() {
        let mut map = ObjectMap::new();
        map.insert(KeyString::from("id"), Value::Integer(1));

        let error =
            validate_tracking_state(&map, &["id".to_owned(), "name".to_owned()], chrono_tz::UTC)
                .expect_err("validation error");

        assert!(error.contains("name"));
    }

    #[test]
    fn prepare_metadata_path_creates_parent_directory() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let path = temp_dir.path().join("nested").join("tracking.json");
        let path = path.to_str().expect("utf-8 path");

        prepare_metadata_path(path).expect("prepare metadata path");

        assert!(temp_dir.path().join("nested").is_dir());
    }

    #[test]
    fn prepare_metadata_path_rejects_empty_path() {
        let error = match prepare_metadata_path("") {
            Err(error) => error,
            Ok(_) => panic!("expected empty path error"),
        };

        assert!(matches!(error, OdbcError::Io { .. }));
    }

    #[test]
    fn ensure_unique_column_names_accepts_distinct_names() {
        ensure_unique_column_names(&["id".to_owned(), "name".to_owned()])
            .expect("distinct column names");
    }

    #[test]
    fn ensure_unique_column_names_errors_on_duplicates() {
        let error = match ensure_unique_column_names(&[
            "id".to_owned(),
            "name".to_owned(),
            "id".to_owned(),
        ]) {
            Err(error) => error,
            Ok(_) => panic!("expected duplicate column names error"),
        };

        assert!(matches!(
            error,
            OdbcError::DuplicateColumnNames { columns } if columns == vec!["id".to_owned()]
        ));
    }

    #[test]
    fn save_params_overwrites_existing_checkpoint() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let path = temp_dir.path().join("tracking.json");
        let path = path.to_str().expect("utf-8 path");

        let mut first = ObjectMap::new();
        first.insert(KeyString::from("id"), Value::Integer(1));
        save_params(path, &first).expect("first save");

        let mut second = ObjectMap::new();
        second.insert(KeyString::from("id"), Value::Integer(2));
        save_params(path, &second).expect("second save should overwrite");

        let saved: ObjectMap =
            serde_json::from_reader(File::open(path).expect("open checkpoint")).expect("parse");
        assert_eq!(saved.get("id"), Some(&Value::Integer(2)));
    }

    #[test]
    fn ensure_unique_column_names_errors_on_multiple_duplicates() {
        let error = match ensure_unique_column_names(&[
            "id".to_owned(),
            "name".to_owned(),
            "id".to_owned(),
            "name".to_owned(),
        ]) {
            Err(error) => error,
            Ok(_) => panic!("expected duplicate column names error"),
        };

        assert!(matches!(
            error,
            OdbcError::DuplicateColumnNames { columns }
                if columns == vec!["id".to_owned(), "name".to_owned()]
        ));
    }
}
