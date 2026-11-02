//! Incremental Parquet batching.
//!
//! Turns a stream of `(partition key, chunk of events)` pairs into a stream of
//! finished Parquet files. Every chunk is encoded as soon as it arrives, so a
//! file's memory footprint is its compressed output plus one chunk rather than
//! the uncompressed events of the whole file. A file is completed when its
//! encoded size or row count reaches the configured limit, when it has been
//! open for longer than the timeout, or when the input ends.

use std::{collections::HashMap, hash::Hash, time::Duration};

use async_stream::stream;
use bytes::Bytes;
use futures::{Stream, StreamExt, future::poll_fn};
use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
    codecs::encoding::{ParquetFileWriter, ParquetSerializer, ParquetWriteError},
    config::telemetry,
    finalization::{EventFinalizers, Finalizable},
    internal_event::{ComponentEventsDropped, UNINTENTIONAL},
    request_metadata::GroupedCountByteSize,
    stream::ExpirationQueue,
    time::KeyedTimer,
};

use crate::{
    codecs::Transformer,
    event::Event,
    internal_events::SinkRequestBuildError,
    sinks::util::{metadata::RequestMetadataBuilder, request_builder::EncodeResult},
};

/// Limits that complete a Parquet file.
#[derive(Clone, Copy, Debug)]
pub struct ParquetBatcherSettings {
    /// Complete the file once its encoded size reaches this many bytes.
    pub max_bytes: usize,
    /// Complete the file once it holds this many rows.
    pub max_events: usize,
    /// Complete the file this long after its first chunk was written.
    pub timeout: Duration,
}

/// A finished Parquet file and the bookkeeping needed to turn it into a request.
#[derive(Debug)]
pub struct ParquetFile {
    /// The complete Parquet file.
    pub payload: Bytes,
    /// Finalizers of every event in the file.
    pub finalizers: EventFinalizers,
    /// Number of events in the file.
    pub event_count: usize,
    /// In-memory size of the events in the file.
    pub events_byte_size: usize,
    /// Estimated JSON size of the events in the file, after transformation.
    pub json_size: GroupedCountByteSize,
}

impl ParquetFile {
    /// Builds the request metadata and encode result for this file.
    pub fn into_encode_result(
        self,
    ) -> (EventFinalizers, RequestMetadataBuilder, EncodeResult<Bytes>) {
        let builder = RequestMetadataBuilder::new(
            self.event_count,
            self.events_byte_size,
            self.json_size.clone(),
        );
        let result = EncodeResult::uncompressed(self.payload, self.json_size);
        (self.finalizers, builder, result)
    }
}

struct OpenFile {
    writer: ParquetFileWriter,
    finalizers: EventFinalizers,
    event_count: usize,
    events_byte_size: usize,
    json_size: GroupedCountByteSize,
}

impl OpenFile {
    fn new(writer: ParquetFileWriter) -> Self {
        Self {
            writer,
            finalizers: EventFinalizers::default(),
            event_count: 0,
            events_byte_size: 0,
            json_size: telemetry().create_request_count_byte_size(),
        }
    }

    fn is_full(&self, settings: &ParquetBatcherSettings) -> bool {
        self.writer.encoded_size() >= settings.max_bytes
            || self.writer.rows() >= settings.max_events
    }
}

/// A chunk of events after transformation, with its bookkeeping split off.
struct Chunk {
    events: Vec<Event>,
    finalizers: EventFinalizers,
    events_byte_size: usize,
    json_size: GroupedCountByteSize,
}

impl Chunk {
    fn new(transformer: &Transformer, mut events: Vec<Event>) -> Self {
        let finalizers = events.take_finalizers();
        let mut json_size = telemetry().create_request_count_byte_size();
        let mut events_byte_size = 0;
        for event in &mut events {
            transformer.transform(event);
            events_byte_size += event.size_of();
            json_size.add_event(event, event.estimated_json_encoded_size_of());
        }
        Self {
            events,
            finalizers,
            events_byte_size,
            json_size,
        }
    }
}

/// Encodes each incoming chunk into the Parquet file for its key and yields files
/// as they complete.
///
/// Encoding runs on the blocking thread pool; chunks are processed one at a time.
pub fn parquet_batched<K, St>(
    input: St,
    serializer: ParquetSerializer,
    transformer: Transformer,
    settings: ParquetBatcherSettings,
) -> impl Stream<Item = (K, ParquetFile)>
where
    K: Eq + Hash + Clone + Send + 'static,
    St: Stream<Item = (K, Vec<Event>)> + Send,
{
    stream! {
        let mut input = Box::pin(input.fuse());
        let mut files: HashMap<K, OpenFile> = HashMap::new();
        let mut timer = ExpirationQueue::new(settings.timeout);

        loop {
            let step = tokio::select! {
                biased;

                expired = poll_fn(|cx| timer.poll_expired(cx)), if !timer.is_empty() => {
                    Step::Expired(expired)
                }

                next = input.next() => match next {
                    Some((key, events)) => Step::Chunk(key, events),
                    None => Step::Done,
                },
            };

            match step {
                Step::Expired(Some(key)) => {
                    if let Some(file) = files.remove(&key)
                        && let Some(file) = finish_file(file).await
                    {
                        yield (key, file);
                    }
                }
                Step::Expired(None) => {}
                Step::Chunk(key, events) => {
                    let chunk = Chunk::new(&transformer, events);
                    if chunk.events.is_empty() {
                        continue;
                    }

                    let file = match files.remove(&key) {
                        Some(file) => file,
                        None => match serializer
                            .open_file_with_capacity(&chunk.events, output_capacity(&settings))
                        {
                            Ok(writer) => {
                                timer.insert(key.clone());
                                OpenFile::new(writer)
                            }
                            Err(error) => {
                                drop_events(&error, chunk.events.len());
                                continue;
                            }
                        },
                    };

                    match write_chunk(file, chunk).await {
                        Some(file) if file.is_full(&settings) => {
                            timer.remove(&key);
                            if let Some(file) = finish_file(file).await {
                                yield (key, file);
                            }
                        }
                        Some(file) => {
                            files.insert(key, file);
                        }
                        None => {
                            timer.remove(&key);
                        }
                    }
                }
                Step::Done => break,
            }
        }

        timer.clear();
        for (key, file) in files.drain() {
            if let Some(file) = finish_file(file).await {
                yield (key, file);
            }
        }
    }
}

/// Output buffer to reserve per file: the size limit plus room for the last
/// batch to overshoot it, capped so an unbounded limit does not reserve
/// absurd amounts of address space.
fn output_capacity(settings: &ParquetBatcherSettings) -> usize {
    const MAX_RESERVATION: usize = 1024 * 1024 * 1024;
    settings
        .max_bytes
        .saturating_add(settings.max_bytes / 8)
        .min(MAX_RESERVATION)
}

enum Step<K> {
    Expired(Option<K>),
    Chunk(K, Vec<Event>),
    Done,
}

/// Encodes `chunk` into `file`. Returns the file unless the writer failed, in
/// which case the file and everything in it is dropped.
async fn write_chunk(mut file: OpenFile, chunk: Chunk) -> Option<OpenFile> {
    let Chunk {
        events,
        finalizers,
        events_byte_size,
        json_size,
    } = chunk;
    let chunk_len = events.len();

    let (writer, result) = run_blocking(move || {
        let mut writer = file.writer;
        let result = writer.write(&events);
        (writer, result)
    })
    .await;

    match result {
        Ok(_) => {
            file.finalizers.merge(finalizers);
            file.event_count += chunk_len;
            file.events_byte_size += events_byte_size;
            file.json_size += json_size;
        }
        Err(error @ ParquetWriteError::Chunk(_)) => {
            // Only this chunk is lost; its finalizers drop as failed.
            drop_events(&error, chunk_len);
        }
        Err(error @ ParquetWriteError::File(_)) => {
            drop_events(&error, chunk_len + file.event_count);
            return None;
        }
    }

    file.writer = writer;
    Some(file)
}

async fn finish_file(file: OpenFile) -> Option<ParquetFile> {
    let OpenFile {
        writer,
        finalizers,
        event_count,
        events_byte_size,
        json_size,
    } = file;

    match run_blocking(move || writer.finish()).await {
        Ok(payload) => Some(ParquetFile {
            payload: payload.freeze(),
            finalizers,
            event_count,
            events_byte_size,
            json_size,
        }),
        Err(error) => {
            drop_events(&error, event_count);
            None
        }
    }
}

async fn run_blocking<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(result) => result,
        Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
        Err(error) => panic!("Parquet encoding task was cancelled: {error}"),
    }
}

fn drop_events<E: std::fmt::Display>(error: &E, count: usize) {
    emit!(SinkRequestBuildError { error });
    emit!(ComponentEventsDropped::<UNINTENTIONAL> {
        count,
        reason: "Failed to encode events as Parquet.",
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::stream;
    use parquet::file::reader::{FileReader, SerializedFileReader};
    use vector_lib::{
        codecs::encoding::{ParquetSchemaMode, ParquetSerializerConfig},
        event::{BatchNotifier, BatchStatus, LogEvent},
        finalization::EventStatus,
    };
    use vrl::event_path;

    use super::*;

    fn serializer() -> ParquetSerializer {
        ParquetSerializer::new(ParquetSerializerConfig {
            schema_mode: ParquetSchemaMode::AutoInfer,
            ..Default::default()
        })
        .expect("serializer")
    }

    fn settings(max_bytes: usize, max_events: usize, timeout: Duration) -> ParquetBatcherSettings {
        ParquetBatcherSettings {
            max_bytes,
            max_events,
            timeout,
        }
    }

    const FOREVER: Duration = Duration::from_hours(1);

    fn chunk(count: usize, tag: &str) -> Vec<Event> {
        (0..count)
            .map(|i| {
                let mut log = LogEvent::default();
                log.insert(event_path!("message"), format!("{tag} message {i}"));
                log.insert(event_path!("n"), i64::try_from(i).unwrap());
                Event::Log(log)
            })
            .collect()
    }

    fn row_count(payload: &Bytes) -> usize {
        SerializedFileReader::new(payload.clone())
            .expect("valid parquet")
            .metadata()
            .file_metadata()
            .num_rows()
            .try_into()
            .expect("row count")
    }

    async fn collect<St>(
        input: St,
        settings: ParquetBatcherSettings,
    ) -> Vec<(&'static str, ParquetFile)>
    where
        St: Stream<Item = (&'static str, Vec<Event>)> + Send,
    {
        parquet_batched(input, serializer(), Transformer::default(), settings)
            .collect()
            .await
    }

    #[tokio::test]
    async fn accumulates_chunks_per_key_until_input_ends() {
        let input = stream::iter(vec![
            ("a", chunk(10, "a1")),
            ("b", chunk(5, "b1")),
            ("a", chunk(10, "a2")),
        ]);

        let mut files = collect(input, settings(usize::MAX, usize::MAX, FOREVER)).await;
        files.sort_by_key(|(key, _)| *key);

        assert_eq!(files.len(), 2);
        let (key, file) = &files[0];
        assert_eq!(*key, "a");
        assert_eq!(file.event_count, 20);
        assert_eq!(row_count(&file.payload), 20);
        assert!(file.events_byte_size > 0);
        let (key, file) = &files[1];
        assert_eq!(*key, "b");
        assert_eq!(row_count(&file.payload), 5);
    }

    #[tokio::test]
    async fn completes_file_at_max_events() {
        let input = stream::iter(vec![
            ("a", chunk(4, "1")),
            ("a", chunk(4, "2")),
            ("a", chunk(4, "3")),
        ]);

        let files = collect(input, settings(usize::MAX, 8, FOREVER)).await;
        let rows: Vec<usize> = files.iter().map(|(_, f)| row_count(&f.payload)).collect();
        assert_eq!(rows, vec![8, 4]);
    }

    #[tokio::test]
    async fn completes_file_at_max_bytes() {
        let input = stream::iter(vec![("a", chunk(4, "1")), ("a", chunk(4, "2"))]);

        // Any encoded byte exceeds the limit, so each chunk becomes its own file.
        let files = collect(input, settings(1, usize::MAX, FOREVER)).await;
        let rows: Vec<usize> = files.iter().map(|(_, f)| row_count(&f.payload)).collect();
        assert_eq!(rows, vec![4, 4]);
    }

    #[tokio::test(start_paused = true)]
    async fn completes_file_on_timeout() {
        let input = stream::iter(vec![("a", chunk(3, "1"))]).chain(stream::pending());

        let mut files = Box::pin(parquet_batched(
            input,
            serializer(),
            Transformer::default(),
            settings(usize::MAX, usize::MAX, Duration::from_secs(5)),
        ));

        let (key, file) = tokio::time::timeout(Duration::from_mins(1), files.next())
            .await
            .expect("file should complete once the timeout elapses")
            .expect("stream should yield a file");
        assert_eq!(key, "a");
        assert_eq!(row_count(&file.payload), 3);
    }

    #[tokio::test]
    async fn carries_finalizers_of_every_chunk() {
        let (batch_a, receiver_a) = BatchNotifier::new_with_receiver();
        let (batch_b, receiver_b) = BatchNotifier::new_with_receiver();
        let first: Vec<Event> = chunk(2, "1")
            .into_iter()
            .map(|event| event.with_batch_notifier(&batch_a))
            .collect();
        let second: Vec<Event> = chunk(2, "2")
            .into_iter()
            .map(|event| event.with_batch_notifier(&batch_b))
            .collect();
        drop((batch_a, batch_b));

        let files = collect(
            stream::iter(vec![("a", first), ("a", second)]),
            settings(usize::MAX, usize::MAX, FOREVER),
        )
        .await;
        assert_eq!(files.len(), 1);
        let (_, file) = files.into_iter().next().unwrap();
        assert_eq!(file.finalizers.len(), 4);

        file.finalizers.update_status(EventStatus::Delivered);
        drop(file);
        assert_eq!(receiver_a.await, BatchStatus::Delivered);
        assert_eq!(receiver_b.await, BatchStatus::Delivered);
    }

    #[tokio::test]
    async fn chunk_error_drops_only_that_chunk() {
        // The first chunk fixes `n` as an integer column; a chunk where `n` is a
        // string cannot be converted and is dropped, but the file stays open.
        let mut bad = LogEvent::default();
        bad.insert(event_path!("message"), "bad");
        bad.insert(event_path!("n"), "not a number");

        let input = stream::iter(vec![
            ("a", chunk(3, "1")),
            ("a", vec![Event::Log(bad)]),
            ("a", chunk(3, "2")),
        ]);

        let files = collect(input, settings(usize::MAX, usize::MAX, FOREVER)).await;
        assert_eq!(files.len(), 1);
        let (_, file) = &files[0];
        assert_eq!(file.event_count, 6);
        assert_eq!(row_count(&file.payload), 6);
    }
}
