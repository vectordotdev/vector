use crate::sinks::util::sink::ServiceLogic;
use crate::{
    config::{log_schema, SinkContext},
    event::Event,
    sinks::{
        aws_s3::config::Encoding,
        util::{
            buffer::GZIP_FAST,
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
    stream::{BoxStream, FuturesUnordered, StreamExt},
    FutureExt, TryFutureExt,
};
use std::{
    collections::HashMap,
    fmt::Debug,
    io::{self, Write},
    num::NonZeroUsize,
    time::Duration,
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
    sink::StreamSink,
    stream::batcher::Batcher,
};

use super::{config::S3RequestOptions, partitioner::KeyPartitioner, service::S3Request};
use crate::sinks::util::sink::Response;

pub struct S3Sink<S> {
    acker: Option<Acker>,
    service: Option<S>,
    partitioner: Option<KeyPartitioner>,
    batch_size_bytes: Option<NonZeroUsize>,
    batch_size_events: NonZeroUsize,
    batch_timeout: Duration,
    options: S3RequestOptions,
}

impl<S> S3Sink<S> {
    pub fn new(
        cx: SinkContext,
        service: S,
        partitioner: KeyPartitioner,
        batch_size_bytes: Option<NonZeroUsize>,
        batch_size_events: NonZeroUsize,
        batch_timeout: Duration,
        options: S3RequestOptions,
    ) -> Self {
        Self {
            acker: Some(cx.acker()),
            service: Some(service),
            partitioner: Some(partitioner),
            batch_size_events,
            batch_size_bytes,
            batch_timeout,
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
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // All sinks do the same fundamental job: take in events, and ship them
        // out. Empirical testing shows that our number one priority for high
        // throughput needs to be servicing I/O as soon as we possibly can.  In
        // order to do that, we'll spin up a separate task that deals
        // exclusively with I/O, while we deal with everything else here:
        // batching, ordering, and so on.
        let (io_tx, io_rx) = channel(64);
        let service = self
            .service
            .take()
            .expect("same sink should not be run twice");
        let acker = self
            .acker
            .take()
            .expect("same sink should not be run twice");
        let partitioner = self
            .partitioner
            .take()
            .expect("same sink should not be run twice");

        let io = run_io(io_rx, service, acker).in_current_span();
        let _ = tokio::spawn(io);

        let batcher = Batcher::new(
            input,
            partitioner,
            self.batch_timeout,
            self.batch_size_events,
            self.batch_size_bytes,
        );
        pin!(batcher);

        while let Some((key, batch)) = batcher.next().await {
            match key {
                Some(key) => {
                    // We could push this down to the I/O task if we wanted to.
                    let request = build_request(key, batch, &self.options);
                    if io_tx.send(request).await.is_err() {
                        error!(
                            "Sink I/O channel should not be closed before sink itself is closed."
                        );
                        return Err(());
                    }
                }
                // Partitioning failed for one or more events.
                //
                // TODO: this might be where we would insert something like the
                // proposed error handling/dead letter queue stuff; events that
                // _we_ can't handle, but some other component may be able to
                // salvage
                None => {
                    continue;
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
                // Rebind the variable to avoid a bug with the pattern matching
                // in `select!`: https://github.com/tokio-rs/tokio/issues/4076
                let mut req = req;
                let seqno = seq_head;
                seq_head += 1;

                let (tx, rx) = oneshot::channel();

                in_flight.push(rx);

                trace!(
                    message = "Submitting service request.",
                    in_flight_requests = in_flight.len()
                );
                // TODO: This likely need be parameterized, which builds a
                // stronger case for following through with the comment
                // mentioned below.
                let logic = StdServiceLogic::default();
                // TODO: I'm not entirely happy with how we're smuggling
                // batch_size/finalizers this far through, from the finished
                // batch all the way through to the concrete request type...we
                // lifted this code from `ServiceSink` directly, but we should
                // probably treat it like `PartitionBatcher` and shove it into a
                // single, encapsulated type instead.
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

fn build_request(key: String, batch: Vec<Event>, options: &S3RequestOptions) -> S3Request {
    // Generate the filename for this batch, which involves a surprising amount
    // of code.
    let filename = {
        /*
        Since this is generic over the partitioner, for purposes of unit tests,
        we can't get the compiler to let us define a conversion trait such that
        we can get &Event from &P::Item, or I at least don't know how to
        trivially do that.  I'm leaving this snippet here because it embodies
        the prior TODO comment of using the timestamp of the last event in the
        batch rather than the current time.

        Now that I think of it... is that even right?  Do customers want logs
        with timestamps in them related to the last event contained within, or
        do they want timestamps that include when the file was generated and
        dropped into the bucket?  My gut says "time when the log dropped" but
        maybe not...

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
    let key = format!("{}/{}.{}", key, filename, extension);

    // Process our events. This does all of the necessary encoding rule
    // application, as well as encoding and compressing the events.  We're
    // handed back a tidy `Bytes` instance we can send directly to S3.
    let batch_size = batch.len();
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

pub fn process_event_batch(
    batch: Vec<Event>,
    encoding: &EncodingConfig<Encoding>,
    compression: Compression,
) -> (Bytes, EventFinalizers) {
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

    // Build our compressor first, so that we can encode directly into it.
    let mut writer = {
        // This is a best guess, because encoding could add a good chunk of
        // overhead to the raw, in-memory representation of an event, but if
        // we're compressing, then we should end up net below the capacity.
        let buffer = Vec::with_capacity(1_024);
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

    let mut finalizers = EventFinalizers::default();

    // Now encode each item into the writer.
    for mut event in batch {
        finalizers.merge(event.take_finalizers());

        let _ = encode_event(event, encoding, &mut writer)
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

    use crate::sinks::aws_s3::config::S3Options;
    use vector_core::partition::Partitioner;

    use super::*;

    #[derive(Clone, Default)]
    struct TestPartitioner;

    impl Partitioner for TestPartitioner {
        type Item = Event;
        type Key = &'static str;

        fn partition(&self, _: &Self::Item) -> Self::Key {
            "key"
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
        let partitioner = TestPartitioner::default();

        let event = "hello world".into();
        let partition_key = partitioner.partition(&event).to_string();
        let finished_batch = vec![event];

        let settings = S3RequestOptions {
            bucket: "bucket".into(),
            filename_time_format: "date".into(),
            filename_append_uuid: false,
            filename_extension: Some("ext".into()),
            api_options: S3Options::default(),
            encoding: Encoding::Text.into(),
            compression: Compression::None,
        };
        let req = build_request(partition_key.clone(), finished_batch.clone(), &settings);
        assert_eq!(req.key, "key/date.ext");

        let settings = S3RequestOptions {
            filename_extension: None,
            ..settings
        };
        let req = build_request(partition_key.clone(), finished_batch.clone(), &settings);
        assert_eq!(req.key, "key/date.log");

        let settings = S3RequestOptions {
            compression: Compression::gzip_default(),
            ..settings
        };
        let req = build_request(partition_key.clone(), finished_batch.clone(), &settings);
        assert_eq!(req.key, "key/date.log.gz");

        let settings = S3RequestOptions {
            filename_append_uuid: true,
            ..settings
        };
        let req = build_request(partition_key, finished_batch, &settings);
        assert_ne!(req.key, "key/date.log.gz");
    }
}
