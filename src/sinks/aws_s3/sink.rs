use crate::sinks::aws_s3::config::S3RequestOptions;
use crate::{
    config::log_schema,
    event::Event,
    sinks::{
        aws_s3::config::Encoding,
        s3_common::{
            service::S3Request,
            sink::{process_event_batch, S3EventEncoding, S3RequestBuilder},
        },
        util::encoding::EncodingConfiguration,
    },
};
use chrono::Utc;
use std::io::{self, Write};
use uuid::Uuid;

impl S3EventEncoding for S3RequestOptions {
    fn encode_event(&mut self, mut event: Event, mut writer: &mut dyn Write) -> io::Result<()> {
        self.encoding.apply_rules(&mut event);

        let log = event.into_log();
        match self.encoding.codec() {
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
}

impl S3RequestBuilder for S3RequestOptions {
    fn build_request(&mut self, key: String, batch: Vec<Event>) -> S3Request {
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
            let formatted_ts = Utc::now().format(self.filename_time_format.as_str());

            if self.filename_append_uuid {
                let uuid = Uuid::new_v4();
                format!("{}-{}", formatted_ts, uuid.to_hyphenated())
            } else {
                formatted_ts.to_string()
            }
        };

        let extension = self
            .filename_extension
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.compression.extension().into());
        let key = format!("{}/{}.{}", key, filename, extension);

        // Process our events. This does all of the necessary encoding rule
        // application, as well as encoding and compressing the events.  We're
        // handed back a tidy `Bytes` instance we can send directly to S3.
        let batch_size = batch.len();
        let (body, finalizers) = process_event_batch(batch, self, self.compression);

        debug!(
            message = "Sending events.",
            bytes = ?body.len(),
            bucket = ?self.bucket,
            key = ?key
        );

        S3Request {
            body,
            bucket: self.bucket.clone(),
            key,
            content_encoding: self.compression.content_encoding(),
            options: self.api_options.clone(),
            batch_size,
            finalizers,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, io::Cursor};

    use crate::{sinks::s3_common::config::S3Options, sinks::util::encoding::EncodingConfig};
    use vector_core::partition::Partitioner;

    use super::*;
    use crate::sinks::util::Compression;

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
        let mut request_options = S3RequestOptions {
            encoding: EncodingConfig::from(Encoding::Text),
            ..request_options()
        };
        let _ = request_options
            .encode_event(message.clone().into(), &mut writer)
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
        let mut request_options = S3RequestOptions {
            encoding: EncodingConfig::from(Encoding::Ndjson),
            ..request_options()
        };
        let _ = request_options
            .encode_event(event, &mut writer)
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
        let mut request_options = S3RequestOptions {
            encoding: encoding_config,
            ..request_options()
        };
        let _ = request_options
            .encode_event(event, &mut writer)
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

        let mut settings = request_options();
        let req = settings.build_request(partition_key.clone(), finished_batch.clone());
        assert_eq!(req.key, "key/date.ext");

        let mut settings = S3RequestOptions {
            filename_extension: None,
            ..request_options()
        };
        let req = settings.build_request(partition_key.clone(), finished_batch.clone());
        assert_eq!(req.key, "key/date.log");

        let mut settings = S3RequestOptions {
            compression: Compression::gzip_default(),
            ..settings
        };
        let req = settings.build_request(partition_key.clone(), finished_batch.clone());
        assert_eq!(req.key, "key/date.log.gz");

        let mut settings = S3RequestOptions {
            filename_append_uuid: true,
            ..settings
        };
        let req = settings.build_request(partition_key, finished_batch);
        assert_ne!(req.key, "key/date.log.gz");
    }

    fn request_options() -> S3RequestOptions {
        S3RequestOptions {
            bucket: "bucket".into(),
            filename_time_format: "date".into(),
            filename_append_uuid: false,
            filename_extension: Some("ext".into()),
            api_options: S3Options::default(),
            encoding: Encoding::Text.into(),
            compression: Compression::None,
        }
    }
}
