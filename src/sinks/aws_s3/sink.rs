use std::io;

use bytes::Bytes;
use chrono::{FixedOffset, Utc};
use uuid::Uuid;
use vector_lib::{codecs::EncoderKind, event::Finalizable, request_metadata::RequestMetadata};

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

    fn build_request(
        &self,
        mut s3metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let ssekms_key_id = s3metadata.partition_key.ssekms_key_id.clone();
        let mut s3_options = self.api_options.clone();
        s3_options.ssekms_key_id = ssekms_key_id;

        s3metadata.s3_key = self.finalize_s3_key(&s3metadata.partition_key, &s3metadata.s3_key);

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

impl S3RequestOptions {
    fn finalize_s3_key(&self, partition_key: &S3PartitionKey, current_key: &str) -> String {
        if partition_key.is_full_key {
            return current_key.to_owned();
        }

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

        let extension = self
            .filename_extension
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.compression.extension().into());

        format_s3_key(current_key, &filename, &extension)
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
    use vector_lib::codecs::TextSerializerConfig;
    use vector_lib::codecs::encoding::{Framer, FramingConfig};

    use crate::codecs::Encoder as VectorEncoder;

    use super::*;

    fn options(filename_extension: Option<String>, append_uuid: bool) -> S3RequestOptions {
        let framer = FramingConfig::NewlineDelimited.build();
        let serializer = TextSerializerConfig::default().build().into();
        let encoder =
            EncoderKind::Framed(Box::new(VectorEncoder::<Framer>::new(framer, serializer)));

        S3RequestOptions {
            bucket: "bucket".to_string(),
            filename_time_format: "%s".to_string(),
            filename_append_uuid: append_uuid,
            filename_extension,
            api_options: S3Options::default(),
            encoder: (Transformer::default(), encoder),
            compression: Compression::None,
            filename_tz_offset: None,
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

    #[test]
    fn finalize_uses_full_key_verbatim_when_is_full_key() {
        let opts = options(Some("ignored".to_string()), true);
        let partition = S3PartitionKey {
            key_prefix: "logs/h-1/2026-06-04.log".to_string(),
            is_full_key: true,
            ssekms_key_id: None,
        };

        let finalized = opts.finalize_s3_key(&partition, &partition.key_prefix);

        assert_eq!(finalized, "logs/h-1/2026-06-04.log");
    }

    #[test]
    fn finalize_appends_filename_when_not_full_key() {
        let opts = options(Some("log".to_string()), false);
        let partition = S3PartitionKey {
            key_prefix: "prefix/".to_string(),
            is_full_key: false,
            ssekms_key_id: None,
        };

        let finalized = opts.finalize_s3_key(&partition, &partition.key_prefix);

        assert!(finalized.starts_with("prefix/"));
        assert!(finalized.ends_with(".log"));
        // No UUID because append_uuid=false; just the timestamp + extension.
        assert!(!finalized.contains('-'));
    }
}
