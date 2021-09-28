use crate::{
    event::Event,
    sinks::{
        s3_common::{config::S3Options, service::S3Request},
        util::{
            encoding::{EncodingConfig, StandardEncodings},
            Compression, RequestBuilder,
        },
    },
};
use bytes::Bytes;
use chrono::Utc;
use uuid::Uuid;
use vector_core::event::{EventFinalizers, Finalizable};

#[derive(Clone)]
pub struct S3RequestOptions {
    pub bucket: String,
    pub filename_time_format: String,
    pub filename_append_uuid: bool,
    pub filename_extension: Option<String>,
    pub api_options: S3Options,
    pub encoding: EncodingConfig<StandardEncodings>,
    pub compression: Compression,
}

impl RequestBuilder<(String, Vec<Event>)> for S3RequestOptions {
    type Metadata = (String, usize, EventFinalizers);
    type Events = Vec<Event>;
    type Payload = Bytes;
    type Request = S3Request;
    type SplitError = ();

    fn split_input(
        &self,
        input: (String, Vec<Event>),
    ) -> Result<(Self::Metadata, Self::Events), Self::SplitError> {
        let (partition_key, mut events) = input;
        let finalizers = events.take_finalizers();

        Ok(((partition_key, events.len(), finalizers), events))
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (key, batch_size, finalizers) = metadata;

        let filename = {
            let formatted_ts = Utc::now().format(self.filename_time_format.as_str());

            self.filename_append_uuid
                .then(|| format!("{}-{}", formatted_ts, Uuid::new_v4().to_hyphenated()))
                .unwrap_or_else(|| formatted_ts.to_string())
        };

        let extension = self
            .filename_extension
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.compression.extension().into());
        let key = format!("{}/{}.{}", key, filename, extension);

        trace!(
            message = "Sending events.",
            bytes = ?payload.len(),
            events_len = ?batch_size,
            bucket = ?self.bucket,
            key = ?key
        );

        S3Request {
            body: payload,
            bucket: self.bucket.clone(),
            key,
            content_encoding: self.compression.content_encoding(),
            options: self.api_options.clone(),
            batch_size,
            finalizers,
        }
    }
}
