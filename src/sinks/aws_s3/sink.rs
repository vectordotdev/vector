use std::{
    fmt::Display,
    io::{self, Write},
};

use crate::{
    config::{log_schema, SinkContext},
    event::Event,
    sinks::{
        aws_s3::config::Encoding,
        util::{
            buffer::{
                partition::{PartitionBatcher, PartitionFinishedBatch, Partitioner},
                GZIP_FAST,
            },
            encoding::{EncodingConfig, EncodingConfiguration},
            Compression,
        },
    },
};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use flate2::write::GzEncoder;
use futures::stream::BoxStream;
use tokio::{pin, select, sync::mpsc::channel};
use tower::Service;
use uuid::Uuid;
use vector_core::{buffers::Acker, event::EventFinalizers, sink::StreamSink};

use super::{
    config::{S3Options, S3RequestOptions},
    service::S3Request,
};

pub struct S3Sink<S, P>
where
    P: Partitioner,
{
    acker: Acker,
    service: S,
    batcher: PartitionBatcher<P>,
    options: S3RequestOptions,
}

impl<S, P> S3Sink<S, P>
where
    S: Service<S3Request>,
    P: Partitioner,
{
    pub fn new(
        cx: SinkContext,
        service: S,
        batcher: PartitionBatcher<P>,
        options: S3RequestOptions,
    ) -> Self {
        Self {
            acker: cx.acker(),
            service,
            batcher,
            options,
        }
    }
}

#[async_trait]
impl<S, P> StreamSink for S3Sink<S, P>
where
    S: Service<S3Request> + Send,
    P: Partitioner + Send,
    P::Key: Send,
    P::Item: Send,
{
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        pin!(input);

        // All sinks do the same fundamental job: take in events, and ship them out.  Empirical
        // testing shows that our number one priority for high throughput needs to be servicing I/O
        // as soon as we possibly can.  In order to do that, we'll spin up a separate task that
        // deals exclusively with I/O, while we deal with everything else here: batching, ordering,
        // and so on.
        let (io_tx, io_rx) = channel(32);
        let _ = tokio::spawn(async move {
            run_io(io_rx, self.service);
        });

        let mut closed = false;
        loop {
            select! {
                // This explicitly enables biasing, which ensures the branches in this select block
                // are polled in top-down order.  We do tghis to ensure that we send ready batches
                // as soon as possible to keep I/O saturated to the best of our ability.
                biased;

                // Batches ready to send.
                Some(batches) = self.batcher.get_ready_batches() => {

                }

                // Take in new events.
                maybe_event = input.next() => {
                    if let Some(event) = maybe_event {

                    } else {
                        // We're not going to get any more events from our input stream, but we
                        // might have outstanding batches.  Thus, we mark ourselves and the batch as
                        // closed, and the next time we're here, if we're already closed, then we
                        // forcefully break out of the loop.  This ensures that we make at least one
                        // more iteration of the loop, where our next call to `get_ready_batches`
                        // will return all the remaining batches regardless of size or expiration.
                         if !closed {
                             closed = true;
                         } else {
                             break;
                         }
                    }
                }
            }
        }

        Ok(())
    }
}

async fn run_io<S>(rx: Receiver<S3Request>, service: S)
where
    S: Service<S3Request>,
{
}

fn build_request<P>(
    batch: PartitionFinishedBatch<P>,
    time_format: String,
    extension: Option<String>,
    uuid: bool,
    encoding: EncodingConfig<Encoding>,
    compression: Compression,
    bucket: String,
    options: S3Options,
) -> S3Request
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
        let formatted_ts = Utc::now().format(&time_format);

        if uuid {
            let uuid = Uuid::new_v4();
            format!("{}-{}", formatted_ts, uuid.to_hyphenated())
        } else {
            formatted_ts.to_string()
        }
    };

    let extension = extension.unwrap_or_else(|| compression.extension().into());
    let key = format!("{}{}.{}", batch.key(), filename, extension);

    // Process our events. This does all of the necessary encoding rule application, as well as
    // encoding and compressing the events.  We're handed back a tidy `Bytes` instance we can send
    // directly to S3.
    let (body, finalizers) = process_event_batch(batch, encoding, compression);

    debug!(
        message = "Sending events.",
        bytes = ?body.len(),
        bucket = ?bucket,
        key = ?key
    );

    S3Request {
        body,
        bucket,
        key,
        content_encoding: compression.content_encoding(),
        options,
    }
}

pub fn process_event_batch<P>(
    batch: PartitionFinishedBatch<P>,
    encoding: EncodingConfig<Encoding>,
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
                Writer::Plain(inner_buf) => Ok(()),
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
        let _ = encode_event(event.into(), &encoding, &mut writer)
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
    writer: &mut dyn Write,
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

    use crate::sinks::util::{
        buffer::partition::{BatchPushResult, PartitionInFlightBatch, Partitioner},
        BatchSize,
    };

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
        let batch = PartitionInFlightBatch::<TestPartitioner>::new(batch_size);

        let event = "hello world".into();
        assert_eq!(batch.push(event), BatchPushResult::Success(false));

        let finished_batch = batch.finish("key");
        let (buf, _finalizers) =
            process_event_batch(finished_batch, Encoding::Text.into(), Compression::None);

        let req = build_request(
            finished_batch.clone(),
            "date".into(),
            Some("ext".into()),
            false,
            Encoding::Text.into(),
            Compression::None,
            "bucket".into(),
            S3Options::default(),
        );
        assert_eq!(req.key(), "key/date.ext");

        let req = build_request(
            finished_batch.clone(),
            "date".into(),
            None,
            false,
            Encoding::Text.into(),
            Compression::None,
            "bucket".into(),
            S3Options::default(),
        );
        assert_eq!(req.key(), "key/date.log");

        let req = build_request(
            finished_batch.clone(),
            "date".into(),
            None,
            false,
            Encoding::Text.into(),
            Compression::gzip_default(),
            "bucket".into(),
            S3Options::default(),
        );
        assert_eq!(req.key(), "key/date.log.gz");

        let req = build_request(
            finished_batch,
            "date".into(),
            None,
            true,
            Encoding::Text.into(),
            Compression::gzip_default(),
            "bucket".into(),
            S3Options::default(),
        );
        assert_ne!(req.key(), "key/date.log.gz");
    }
}
