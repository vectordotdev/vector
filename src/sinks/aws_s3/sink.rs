use crate::{
    config::log_schema,
    event::Event,
    sinks::{
        aws_s3::config::Encoding,
        s3_common::{
            config::S3Options,
            service::S3Request,
            sink::S3RequestBuilder,
        },
        util::{
            encoding::{EncodingConfig, EncodingConfiguration},
            Compression,
        },
    },
};
use chrono::Utc;
use std::io::{self, Write};
use uuid::Uuid;

#[derive(Clone)]
pub struct S3RequestOptions {
    pub bucket: String,
    pub filename_time_format: String,
    pub filename_append_uuid: bool,
    pub filename_extension: Option<String>,
    pub api_options: S3Options,
    pub encoding: EncodingConfig<Encoding>,
    pub compression: Compression,
}

impl S3RequestBuilder for S3RequestOptions {
    fn compression(&self) -> Compression {
        self.compression
    }

    fn build_request(&self, key: String, batch: Vec<Event>) -> S3Request {
        let filename = {
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
        
        // TODO: I really want this to not have to be a free function for code sharing, but it's a
        // little incestuous with wanting to be able to access both the compression config as well
        // as the rules for encoding... maybe `process_event_batch` could get moved as an included
        // method to the `S3RequestBuilder` trait itself?
        let (body, finalizers) = self.process_event_batch(batch);

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

    fn encode_event(&self, mut event: Event, mut writer: &mut dyn Write) -> io::Result<()> {
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
