use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    io::{self, Write},
};

use crate::sinks::util::sink::ServiceLogic;
use crate::{
    config::{log_schema, SinkContext},
    event::Event,
    sinks::{
        aws_s3::config::Encoding,
        util::{
            buffer::{
                partition::{PartitionBatcher, PartitionFinishedBatch},
                GZIP_FAST,
            },
            encoding::{EncodingConfig, EncodingConfiguration},
            sink::StdServiceLogic,
            Compression,
        },
    },
};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use flate2::write::GzEncoder;
use futures::{
    future::ready,
    stream::{BoxStream, FuturesUnordered, StreamExt},
    FutureExt, TryFutureExt,
};
use tokio::{
    pin, select,
    sync::{
        mpsc::{channel, Receiver},
        oneshot,
    },
};
use tower::{Service, ServiceExt};
use tracing_futures::Instrument;
use uuid::Uuid;
use vector_core::{
    buffers::Acker,
    event::{EventFinalizers, Finalizable},
    partition::Partitioner,
    sink::StreamSink,
};

use super::{config::S3RequestOptions, partitioner::KeyPartitioner, service::S3Request};
use crate::sinks::util::sink::Response;

pub struct S3Sink<S> {
    acker: Option<Acker>,
    service: Option<S>,
    batcher: PartitionBatcher<KeyPartitioner>,
    options: S3RequestOptions,
}

impl<S> S3Sink<S> {
    pub fn new(
        cx: SinkContext,
        service: S,
        batcher: PartitionBatcher<KeyPartitioner>,
        options: S3RequestOptions,
    ) -> Self {
        Self {
            acker: Some(cx.acker()),
            service: Some(service),
            batcher,
            options,
        }
    }
}

#[async_trait]
impl<S> StreamSink for S3Sink<S>
where
    S: Service<S3Request> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: Response + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        // All sinks do the same fundamental job: take in events, and ship them out.  Empirical
        // testing shows that our number one priority for high throughput needs to be servicing I/O
        // as soon as we possibly can.  In order to do that, we'll spin up a separate task that
        // deals exclusively with I/O, while we deal with everything else here: batching, ordering,
        // and so on.
        let (io_tx, io_rx) = channel(64);
        let service = self
            .service
            .take()
            .expect("same sink should not be run twice");
        let acker = self
            .acker
            .take()
            .expect("same sink should not be run twice");

        let io = run_io(io_rx, service, acker).in_current_span();
        let _ = tokio::spawn(async move {
            io.await;
        });

        let mut closed = false;
        let mut queued_event = None;
        let mut iter = 0;
        loop {
            iter += 1;

            select! {
                // This explicitly enables biasing, which ensures the branches in this select block
                // are polled in top-down order.  We do this to ensure that we send ready batches
                // as soon as possible to keep I/O saturated to the best of our ability.
                biased;

                // Batches ready to send.
                //
                // TODO: We may want to reconsider running this on an interval in the future because
                // right now we'll be checking it after every single event we receive on our input
                // stream, and it's not exactly the _fastest_ function.  Something like a 10-25ms
                // flush interval could be more than enough batching when subject to high ingest
                // rate e.g. tens or hundreds of thousands of events per second.
                Some(batches) = self.batcher.get_ready_batches() => {
                    trace!("{} -> got {} batches to flush", iter, batches.len());
                    for batch in batches {
                        let batch_key = batch.key().to_string();
                        let request = build_request(batch, &self.options);
                        if let Err(_) = io_tx.send(request).await {
                            // TODO: change this to "error! + return Err" after initial testing/debugging
                            trace!("{} -> sink I/O channel should not be closed before sink itself is closed", iter);
                            panic!("boom 1")
                        }

                        trace!("{} -> sent batch '{}' to I/O task", iter, batch_key);
                    }
                }

                // Queued event that was previously diverted because we were unable to insert it to
                // a batch.  Try again now after flushing batches.
                Some(event) = ready(queued_event.take()) => {
                    // If we're able to get back the event after trying to push it into a batch, it
                    // means we didn't successfully insert the event.  This should not be possible,
                    // as we only queue an event for reinsertion if the batch push failed for a
                    // non-error reason.  If it fails for a non-error reason, that implies that the
                    // batch is full, and that based on our biased select block, any ready batches
                    // should be cleared by the time this branch executes.
                    if let Some(_) = self.batcher.push(event).into_inner() {
                        // TODO: change this to "error! + return Err" after initial testing/debugging
                        trace!("{} -> wasn't able to push queued event into batch after flushing ready batches; this should be impossible", iter);
                        panic!("boom 2");
                    }

                    trace!("{} -> pushed queued event into batch", iter);
                }

                // Take in new events.
                maybe_event = input.next() => {
                    if let Some(event) = maybe_event {
                        // When we receive an event, we aren't yet sure which batch it's going to
                        // land in, based on the configured partitioning, so we might run into a
                        // situation where a batch is actually full and we need to flush it first.
                        //
                        // We'll temporarily hold that event in a side buffer -- `queued_event` --
                        // so that we can forcefully continue the loop, to let our prioritized
                        // "flush buffers" branch run, clearing out the full batch.  After that
                        // happens, we'll push that queued event into a batch.
                        let result = self.batcher.push(event);
                        if result.should_flush() {
                            trace!("{} -> batch full, continuing to force flush", iter);
                            queued_event = result.into_inner();
                            continue;
                        }
                    } else {
                        // We're not going to get any more events from our input stream, but we
                        // might have outstanding batches.  Thus, we mark ourselves and the batch as
                        // closed, and the next time we're here, if we're already closed, then we
                        // forcefully break out of the loop.  This ensures that we make at least one
                        // more iteration of the loop, where our next call to `get_ready_batches`
                        // will return all the remaining batches regardless of size or expiration.
                         if !closed {
                             trace!("{} -> stream closed, marking batcher closed and flushing", iter);
                             closed = true;
                             self.batcher.close();
                         } else {
                             trace!("{} -> stream completed, post-close flush complete, ending", iter);
                             break;
                         }
                    }
                }
            }
        }

        Ok(())
    }
}

async fn run_io<S>(mut rx: Receiver<S3Request>, mut service: S, acker: Acker)
where
    S: Service<S3Request>,
    S::Future: Send + 'static,
    S::Response: Response + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    let in_flight = FuturesUnordered::new();
    let mut pending_acks = HashMap::new();
    let mut seq_head: u64 = 0;
    let mut seq_tail: u64 = 0;

    pin!(in_flight);

    loop {
        select! {
            Some(req) = rx.recv() => {
                // Rebind the variable to avoid a bug with the pattern matching in `select!`:
                // https://github.com/tokio-rs/tokio/issues/4076
                let mut req = req;
                let seqno = seq_head;
                seq_head += 1;

                let (tx, rx) = oneshot::channel();

                in_flight.push(rx);

                trace!(
                    message = "Submitting service request.",
                    in_flight_requests = in_flight.len()
                );
                // TODO: This likely need be parameterized, which builds a stronger case for
                // following through with the comment mentioned below.
                let logic = StdServiceLogic::default();
                // TODO: I'm not entirely happy with how we're smuggling batch_size/finalizers
                // this far through, from the finished batch all the way through to the concrete
                // request type...we lifted this code from `ServiceSink` directly, but we should
                // probably treat it like `PartitionBatcher` and shove it into a single,
                // encapsulated type instead.
                let batch_size = req.batch_size;
                let finalizers = req.take_finalizers();

                let svc = service.ready().await.expect("should not get error when waiting for svc readiness");
                let fut = svc.call(req)
                    .err_into()
                    .map(move |result| {
                        logic.update_finalizers(result, finalizers);

                        // If the rx end is dropped we still completed
                        // the request so this is a weird case that we can
                        // ignore for now.
                        let _ = tx.send((seqno, batch_size));
                    })
                    .instrument(info_span!("request", request_id = %seqno));
                tokio::spawn(fut);
            },

            Some(Ok((seqno, batch_size))) = in_flight.next() => {
                trace!("pending batch {} finished (n={})", seqno, batch_size);
                pending_acks.insert(seqno, batch_size);

                let mut num_to_ack = 0;
                while let Some(ack_size) = pending_acks.remove(&seq_tail) {
                    num_to_ack += ack_size;
                    seq_tail += 1
                }
                trace!(message = "Acking events.", acking_num = num_to_ack);
                acker.ack(num_to_ack);
            },

            else => break
        }
    }
}

fn build_request<P>(batch: PartitionFinishedBatch<P>, options: &S3RequestOptions) -> S3Request
where
    P: Partitioner,
    P::Key: Display,
    P::Item: Into<Event>,
{
    // Generate the filename for this batch, which involves a surprising amount of code.
    let filename = {
        /*
        Since this is generic over the partitioner, for purposes of unit tests, we can't get the compiler to
        let us define a conversion trait such that we can get &Event from &P::Item, or I at least don't know
        how to trivially do that.  I'm leaving this snippet here because it embodies the prior TODO comment
        of using the timestamp of the last event in the batch rather than the current time.

        Now that I think of it... is that even right?  Do customers want logs with timestamps in them related
        to the last event contained within, or do they want timestamps that include when the file was generated
        and dropped into the bucket?  My gut says "time when the log dropped" but maybe not...

        let last_event_ts = batch
            .items()
            .iter()
            .last()
            .and_then(|e| match e.into() {
                // If the event has a field called timestamp, in RFC3339 format, use that.
                Event::Log(le) => le
                    .get(log_schema().timestamp_key())
                    .cloned()
                    .and_then(|ts| match ts {
                        Value::Timestamp(ts) => Some(ts),
                        Value::Bytes(buf) => std::str::from_utf8(&buf)
                            .ok()
                            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&Utc)),
                        _ => None,
                    }),
                // TODO: We don't ship metrics to the S3, but if we did, would this be right? or is
                // there an actual field we should be checking similar to above?
                Event::Metric(_) => Some(Utc::now()),
            })
            .unwrap_or_else(|| Utc::now());
        let formatted_ts = last_event_ts.format(&time_format);
        */
        let formatted_ts = Utc::now().format(options.filename_time_format.as_str());

        if options.filename_append_uuid {
            let uuid = Uuid::new_v4();
            format!("{}-{}", formatted_ts, uuid.to_hyphenated())
        } else {
            formatted_ts.to_string()
        }
    };

    let extension = options
        .filename_extension
        .as_ref()
        .cloned()
        .unwrap_or_else(|| options.compression.extension().into());
    let key = format!("{}{}.{}", batch.key(), filename, extension);

    // Process our events. This does all of the necessary encoding rule application, as well as
    // encoding and compressing the events.  We're handed back a tidy `Bytes` instance we can send
    // directly to S3.
    //
    // TODO: we need to do something with these
    let batch_size = batch.items().len();
    let (body, finalizers) = process_event_batch(batch, &options.encoding, options.compression);

    debug!(
        message = "Sending events.",
        bytes = ?body.len(),
        bucket = ?options.bucket,
        key = ?key
    );

    S3Request {
        body,
        bucket: options.bucket.clone(),
        key,
        content_encoding: options.compression.content_encoding(),
        options: options.api_options.clone(),
        batch_size,
        finalizers,
    }
}

pub fn process_event_batch<P>(
    batch: PartitionFinishedBatch<P>,
    encoding: &EncodingConfig<Encoding>,
    compression: Compression,
) -> (Bytes, EventFinalizers)
where
    P: Partitioner,
    P::Item: Into<Event>,
{
    enum Writer {
        Plain(Vec<u8>),
        GzipCompressed(GzEncoder<Vec<u8>>),
    }

    impl Write for Writer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self {
                Writer::Plain(inner_buf) => inner_buf.write(buf),
                Writer::GzipCompressed(writer) => writer.write(buf),
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            match self {
                Writer::Plain(_) => Ok(()),
                Writer::GzipCompressed(writer) => writer.flush(),
            }
        }
    }

    let total_size = batch.total_size();
    let (_key, events, finalizers) = batch.into_parts();

    // Build our compressor first, so that we can encode directly into it.
    let mut writer = {
        // This is a best guess, because encoding could add a good chunk of overhead to the raw,
        // in-memory representation of an event, but if we're compressing, then we should end up
        // net below the capacity.
        let buffer = Vec::with_capacity(total_size);
        match compression {
            Compression::None => Writer::Plain(buffer),
            Compression::Gzip(level) => {
                let level = level.unwrap_or(GZIP_FAST);
                Writer::GzipCompressed(GzEncoder::new(
                    buffer,
                    flate2::Compression::new(level as u32),
                ))
            }
        }
    };

    // Now encode each item into the writer.
    for event in events {
        let _ = encode_event(event.into(), encoding, &mut writer)
            .expect("failed to encode event into writer; this is a bug!");
    }

    // Extract the buffer and push it back in a frozen state.
    let buf = match writer {
        Writer::Plain(buf) => buf.into(),
        Writer::GzipCompressed(writer) => writer
            .finish()
            .expect("gzip writer should not fail to finish")
            .into(),
    };

    (buf, finalizers)
}

fn encode_event(
    mut event: Event,
    encoding: &EncodingConfig<Encoding>,
    mut writer: &mut dyn Write,
) -> io::Result<()> {
    encoding.apply_rules(&mut event);

    let log = event.into_log();
    match encoding.codec() {
        Encoding::Ndjson => {
            let _ = serde_json::to_writer(&mut writer, &log)?;
            writer.write_all(b"\n")
        }
        Encoding::Text => {
            let buf = log
                .get(log_schema().message_key())
                .map(|v| v.as_bytes())
                .unwrap_or_default();
            let _ = writer.write_all(&buf)?;
            writer.write_all(b"\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, io::Cursor};

    use crate::sinks::{
        aws_s3::config::S3Options,
        util::{
            buffer::partition::{BatchPushResult, PartitionInFlightBatch},
            BatchSize,
        },
    };
    use vector_core::partition::Partitioner;

    use super::*;

    #[derive(Clone)]
    struct TestPartitioner;

    impl Partitioner for TestPartitioner {
        type Item = Event;
        type Key = &'static str;

        fn partition(&self, _: &Self::Item) -> Option<Self::Key> {
            None
        }
    }

    #[test]
    fn s3_encode_event_text() {
        let message = "hello world".to_string();
        let mut writer = Cursor::new(Vec::new());
        let _ = encode_event(message.clone().into(), &Encoding::Text.into(), &mut writer)
            .expect("should not have failed to encode event");
        let encoded = writer.into_inner();

        let encoded_message = message + "\n";
        assert_eq!(encoded.as_slice(), encoded_message.as_bytes());
    }

    #[test]
    fn s3_encode_event_ndjson() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");

        let mut writer = Cursor::new(Vec::new());
        let _ = encode_event(event, &Encoding::Ndjson.into(), &mut writer)
            .expect("should not have failed to encode event");
        let encoded = writer.into_inner();
        let map: BTreeMap<String, String> = serde_json::from_slice(encoded.as_slice()).unwrap();

        assert_eq!(map[&log_schema().message_key().to_string()], message);
        assert_eq!(map["key"], "value".to_string());
    }

    #[test]
    fn s3_encode_event_with_removed_key() {
        let encoding_config = EncodingConfig {
            codec: Encoding::Ndjson,
            schema: None,
            only_fields: None,
            except_fields: Some(vec!["key".into()]),
            timestamp_format: None,
        };

        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");

        let mut writer = Cursor::new(Vec::new());
        let _ = encode_event(event, &encoding_config, &mut writer)
            .expect("should not have failed to encode event");
        let encoded = writer.into_inner();
        let map: BTreeMap<String, String> = serde_json::from_slice(encoded.as_slice()).unwrap();

        assert_eq!(map[&log_schema().message_key().to_string()], message);
        assert!(!map.contains_key("key"));
    }

    #[test]
    fn s3_build_request() {
        let batch_size = BatchSize::const_default();
        let mut batch = PartitionInFlightBatch::<TestPartitioner>::new(batch_size);

        let event = "hello world".into();
        assert_eq!(batch.push(event), BatchPushResult::Success(false));

        let finished_batch = batch.finish("key");

        let settings = S3RequestOptions {
            bucket: "bucket".into(),
            filename_time_format: "date".into(),
            filename_append_uuid: false,
            filename_extension: Some("ext".into()),
            api_options: S3Options::default(),
            encoding: Encoding::Text.into(),
            compression: Compression::None,
        };
        let req = build_request(finished_batch.clone(), &settings);
        assert_eq!(req.key, "key/date.ext");

        let settings = S3RequestOptions {
            filename_extension: None,
            ..settings.clone()
        };
        let req = build_request(finished_batch.clone(), &settings);
        assert_eq!(req.key, "key/date.log");

        let settings = S3RequestOptions {
            compression: Compression::gzip_default(),
            ..settings.clone()
        };
        let req = build_request(finished_batch.clone(), &settings);
        assert_eq!(req.key, "key/date.log.gz");

        let settings = S3RequestOptions {
            filename_append_uuid: true,
            ..settings.clone()
        };
        let req = build_request(finished_batch.clone(), &settings);
        assert_ne!(req.key, "key/date.log.gz");
    }
}
