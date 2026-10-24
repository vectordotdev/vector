use std::io;

use bytes::Bytes;
use chrono::{FixedOffset, Utc};
use uuid::Uuid;
use vector_lib::{
    codecs::EncoderKind,
    event::{EventStatus, Finalizable},
    request_metadata::RequestMetadata,
};

use crate::{
    codecs::Transformer,
    event::Event,
    sinks::{
        s3_common::{
            config::S3Options,
            partitioner::S3PartitionKey,
            service::{S3Metadata, S3Request},
        },
        util::{
            Compression, RequestBuilder, metadata::RequestMetadataBuilder,
            request_builder::EncodeResult,
        },
    },
};

#[derive(Clone)]
pub struct S3RequestOptions {
    pub bucket: String,
    pub filename_time_format: String,
    pub filename_append_uuid: bool,
    pub filename_extension: Option<String>,
    pub api_options: S3Options,
    pub encoder: (Transformer, EncoderKind),
    pub compression: Compression,
    pub filename_tz_offset: Option<FixedOffset>,
}

impl RequestBuilder<(S3PartitionKey, Vec<Event>)> for S3RequestOptions {
    type Metadata = S3Metadata;
    type Events = Vec<Event>;
    type Encoder = (Transformer, EncoderKind);
    type Payload = Bytes;
    type Request = S3Request;
    type Error = io::Error; // TODO: this is ugly.

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (S3PartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (partition_key, mut events) = input;
        let builder = RequestMetadataBuilder::from_events(&events);

        let finalizers = events.take_finalizers();
        let s3_key_prefix = partition_key.key_prefix.clone();

        let metadata = S3Metadata {
            partition_key,
            s3_key: s3_key_prefix,
            finalizers,
        };

        (metadata, builder, events)
    }

    fn on_encode_error(&self, metadata: Self::Metadata) {
        // `split_input` took the finalizers off the events, so this is the only owner left. Reject
        // them, otherwise dropping them would report the batch as delivered and the events would be
        // lost without the upstream source ever knowing.
        metadata.finalizers.update_status(EventStatus::Rejected);
    }

    fn build_request(
        &self,
        mut s3metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let filename = {
            let formatted_ts = match self.filename_tz_offset {
                Some(offset) => Utc::now()
                    .with_timezone(&offset)
                    .format(self.filename_time_format.as_str()),
                None => Utc::now()
                    .with_timezone(&Utc)
                    .format(self.filename_time_format.as_str()),
            };

            if self.filename_append_uuid {
                format!("{formatted_ts}-{}", Uuid::new_v4().hyphenated())
            } else {
                formatted_ts.to_string()
            }
        };

        let ssekms_key_id = s3metadata.partition_key.ssekms_key_id.clone();
        let mut s3_options = self.api_options.clone();
        s3_options.ssekms_key_id = ssekms_key_id;

        let extension = self
            .filename_extension
            .as_deref()
            .unwrap_or_else(|| self.compression.extension());

        s3metadata.s3_key = format_s3_key(&s3metadata.s3_key, &filename, extension);

        S3Request {
            body: payload.into_payload(),
            bucket: self.bucket.clone(),
            metadata: s3metadata,
            request_metadata,
            content_encoding: self.compression.content_encoding(),
            options: s3_options,
        }
    }
}

fn format_s3_key(s3_key: &str, filename: &str, extension: &str) -> String {
    if extension.is_empty() {
        format!("{s3_key}{filename}")
    } else {
        format!("{s3_key}{filename}.{extension}")
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use futures::{StreamExt, stream};
    use vector_lib::{
        codecs::encoding::{AvroSerializerConfig, Framer, NewlineDelimitedEncoder},
        event::{BatchNotifier, BatchStatus, LogEvent},
    };

    use super::*;
    use crate::sinks::util::builder::SinkBuilderExt;

    /// An Avro schema requiring a field the test events never carry, so every
    /// encode attempt fails at schema resolution.
    const UNSATISFIABLE_AVRO_SCHEMA: &str = r#"{
        "type": "record",
        "name": "Log",
        "fields": [{ "name": "required_field", "type": "string" }]
    }"#;

    fn request_options(encoder: (Transformer, EncoderKind)) -> S3RequestOptions {
        S3RequestOptions {
            bucket: "test-bucket".to_owned(),
            filename_time_format: "%s".to_owned(),
            filename_append_uuid: false,
            filename_extension: None,
            api_options: S3Options::default(),
            encoder,
            compression: Compression::None,
            filename_tz_offset: None,
        }
    }

    fn failing_encoder() -> (Transformer, EncoderKind) {
        let serializer = AvroSerializerConfig::new(UNSATISFIABLE_AVRO_SCHEMA.to_owned())
            .build()
            .expect("valid Avro schema");
        let encoder = vector_lib::codecs::Encoder::<Framer>::new(
            NewlineDelimitedEncoder::default().into(),
            serializer.into(),
        );

        (
            Transformer::default(),
            EncoderKind::Framed(Box::new(encoder)),
        )
    }

    fn working_encoder() -> (Transformer, EncoderKind) {
        (Transformer::default(), EncoderKind::Framed(Box::default()))
    }

    fn partition_key() -> S3PartitionKey {
        S3PartitionKey {
            key_prefix: "prefix/".to_owned(),
            ssekms_key_id: None,
        }
    }

    #[test]
    fn test_format_s3_key() {
        assert_eq!(
            "s3_key_filename.txt",
            format_s3_key("s3_key_", "filename", "txt")
        );
        assert_eq!("s3_key_filename", format_s3_key("s3_key_", "filename", ""));
    }

    /// Encoding failures must reject the batch instead of letting the dropped
    /// finalizers report a successful delivery upstream.
    #[tokio::test]
    async fn encode_failure_rejects_the_batch() {
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let events = vec![Event::Log(
            LogEvent::from("hello world").with_batch_notifier(&batch),
        )];
        drop(batch);

        let mut requests = stream::iter(vec![(partition_key(), events)]).request_builder(
            NonZeroUsize::new(1).expect("nonzero"),
            request_options(failing_encoder()),
        );

        let result = requests.next().await.expect("one result");
        assert!(result.is_err(), "the encoder is expected to fail");
        drop(requests);

        assert_eq!(receiver.await, BatchStatus::Rejected);
    }

    /// A successful encode must leave the finalizers attached to the request so
    /// the driver still owns the delivery outcome.
    #[tokio::test]
    async fn encode_success_leaves_the_batch_to_the_driver() {
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let events = vec![Event::Log(
            LogEvent::from("hello world").with_batch_notifier(&batch),
        )];
        drop(batch);

        let mut requests = stream::iter(vec![(partition_key(), events)]).request_builder(
            NonZeroUsize::new(1).expect("nonzero"),
            request_options(working_encoder()),
        );

        let mut request = requests.next().await.expect("one result").expect("encoded");
        drop(requests);

        let finalizers = request.take_finalizers();
        finalizers.update_status(EventStatus::Delivered);
        drop(finalizers);

        assert_eq!(receiver.await, BatchStatus::Delivered);
    }
}
