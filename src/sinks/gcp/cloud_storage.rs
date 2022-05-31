use std::{collections::HashMap, convert::TryFrom, io};

use bytes::Bytes;
use chrono::Utc;
use codecs::{
    encoding::{Framer, Serializer},
    CharacterDelimitedEncoder, LengthDelimitedEncoder, NewlineDelimitedEncoder,
};
use http::header::{HeaderName, HeaderValue};
use indoc::indoc;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use snafu::Snafu;
use tower::ServiceBuilder;
use uuid::Uuid;
use vector_core::event::{EventFinalizers, Finalizable};

use crate::{
    codecs::Encoder,
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, SinkDescription,
    },
    event::Event,
    gcp::{GcpAuthConfig, GcpCredentials, Scope},
    http::HttpClient,
    serde::json::to_string,
    sinks::{
        gcs_common::{
            config::{
                build_healthcheck, GcsPredefinedAcl, GcsRetryLogic, GcsStorageClass, BASE_URL,
            },
            service::{GcsRequest, GcsRequestSettings, GcsService},
            sink::GcsSink,
        },
        util::{
            batch::BatchConfig,
            encoding::{
                EncodingConfig, EncodingConfigWithFramingAdapter, StandardEncodings,
                StandardEncodingsWithFramingMigrator, Transformer,
            },
            metadata::{RequestMetadata, RequestMetadataBuilder},
            partitioner::KeyPartitioner,
            request_builder::EncodeResult,
            BulkSizeBasedDefaultBatchSettings, Compression, RequestBuilder, ServiceBuilderExt,
            TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    template::{Template, TemplateParseError},
    tls::{TlsConfig, TlsSettings},
};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GcsHealthcheckError {
    #[snafu(display("key_prefix template parse error: {}", source))]
    KeyPrefixTemplate { source: TemplateParseError },
}

const NAME: &str = "gcp_cloud_storage";

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct GcsSinkConfig {
    bucket: String,
    acl: Option<GcsPredefinedAcl>,
    storage_class: Option<GcsStorageClass>,
    metadata: Option<HashMap<String, String>>,
    key_prefix: Option<String>,
    filename_time_format: Option<String>,
    filename_append_uuid: Option<bool>,
    filename_extension: Option<String>,
    #[serde(flatten)]
    encoding: EncodingConfigWithFramingAdapter<
        EncodingConfig<StandardEncodings>,
        StandardEncodingsWithFramingMigrator,
    >,
    #[serde(default)]
    compression: Compression,
    #[serde(default)]
    batch: BatchConfig<BulkSizeBasedDefaultBatchSettings>,
    #[serde(default)]
    request: TowerRequestConfig,
    #[serde(flatten)]
    auth: GcpAuthConfig,
    tls: Option<TlsConfig>,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

#[cfg(test)]
fn default_config(e: StandardEncodings) -> GcsSinkConfig {
    GcsSinkConfig {
        bucket: Default::default(),
        acl: Default::default(),
        storage_class: Default::default(),
        metadata: Default::default(),
        key_prefix: Default::default(),
        filename_time_format: Default::default(),
        filename_append_uuid: Default::default(),
        filename_extension: Default::default(),
        encoding: EncodingConfig::from(e).into(),
        compression: Compression::gzip_default(),
        batch: Default::default(),
        request: Default::default(),
        auth: Default::default(),
        tls: Default::default(),
        acknowledgements: Default::default(),
    }
}

inventory::submit! {
    SinkDescription::new::<GcsSinkConfig>(NAME)
}

impl GenerateConfig for GcsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            bucket = "my-bucket"
            credentials_path = "/path/to/credentials.json"
            encoding.codec = "ndjson"
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_cloud_storage")]
impl SinkConfig for GcsSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let creds = self
            .auth
            .make_credentials(Scope::DevStorageReadWrite)
            .await?;
        let base_url = format!("{}{}/", BASE_URL, self.bucket);
        let tls = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls, cx.proxy())?;
        let healthcheck = build_healthcheck(
            self.bucket.clone(),
            client.clone(),
            base_url.clone(),
            creds.clone(),
        )?;
        let sink = self.build_sink(client, base_url, creds, cx)?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type())
    }

    fn sink_type(&self) -> &'static str {
        NAME
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
    }
}

impl GcsSinkConfig {
    fn build_sink(
        &self,
        client: HttpClient,
        base_url: String,
        creds: Option<GcpCredentials>,
        cx: SinkContext,
    ) -> crate::Result<VectorSink> {
        let request = self.request.unwrap_with(&TowerRequestConfig {
            rate_limit_num: Some(1000),
            ..Default::default()
        });

        let batch_settings = self.batch.into_batcher_settings()?;

        let partitioner = self.key_partitioner()?;

        let svc = ServiceBuilder::new()
            .settings(request, GcsRetryLogic)
            .service(GcsService::new(client, base_url, creds));

        let request_settings = RequestSettings::new(self)?;

        let sink = GcsSink::new(cx, svc, request_settings, partitioner, batch_settings);

        Ok(VectorSink::from_event_streamsink(sink))
    }

    fn key_partitioner(&self) -> crate::Result<KeyPartitioner> {
        Ok(KeyPartitioner::new(
            Template::try_from(self.key_prefix.as_deref().unwrap_or("date=%F/"))
                .context(KeyPrefixTemplateSnafu)?,
        ))
    }
}

// Settings required to produce a request that do not change per
// request. All possible values are pre-computed for direct use in
// producing a request.
#[derive(Clone, Debug)]
struct RequestSettings {
    acl: Option<HeaderValue>,
    content_type: HeaderValue,
    content_encoding: Option<HeaderValue>,
    storage_class: HeaderValue,
    headers: Vec<(HeaderName, HeaderValue)>,
    extension: String,
    time_format: String,
    append_uuid: bool,
    encoder: (Transformer, Encoder<Framer>),
    compression: Compression,
}

impl RequestBuilder<(String, Vec<Event>)> for RequestSettings {
    type Metadata = (String, EventFinalizers, RequestMetadataBuilder);
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = GcsRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, input: (String, Vec<Event>)) -> (Self::Metadata, Self::Events) {
        let (partition_key, mut events) = input;
        let metadata_builder = RequestMetadata::builder(&events);
        let finalizers = events.take_finalizers();

        ((partition_key, finalizers, metadata_builder), events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (key, finalizers, metadata_builder) = metadata;
        // TODO: pull the seconds from the last event
        let filename = {
            let seconds = Utc::now().format(&self.time_format);

            if self.append_uuid {
                let uuid = Uuid::new_v4();
                format!("{}-{}", seconds, uuid.hyphenated())
            } else {
                seconds.to_string()
            }
        };

        let key = format!("{}{}.{}", key, filename, self.extension);

        let metadata = metadata_builder.build(&payload);
        let body = payload.into_payload();

        GcsRequest {
            key,
            body,
            finalizers,
            settings: GcsRequestSettings {
                acl: self.acl.clone(),
                content_type: self.content_type.clone(),
                content_encoding: self.content_encoding.clone(),
                storage_class: self.storage_class.clone(),
                headers: self.headers.clone(),
            },
            metadata,
        }
    }
}

impl RequestSettings {
    fn new(config: &GcsSinkConfig) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();
        let (framer, serializer) = config.encoding.encoding();
        let framer = match (framer, &serializer) {
            (Some(framer), _) => framer,
            (None, Serializer::Json(_)) => CharacterDelimitedEncoder::new(b',').into(),
            (None, Serializer::Native(_)) => LengthDelimitedEncoder::new().into(),
            (
                None,
                Serializer::Logfmt(_)
                | Serializer::NativeJson(_)
                | Serializer::RawMessage(_)
                | Serializer::Text(_),
            ) => NewlineDelimitedEncoder::new().into(),
        };
        let encoder = Encoder::<Framer>::new(framer, serializer);
        let acl = config
            .acl
            .map(|acl| HeaderValue::from_str(&to_string(acl)).unwrap());
        let content_type = HeaderValue::from_str(encoder.content_type()).unwrap();
        let content_encoding = config
            .compression
            .content_encoding()
            .map(|ce| HeaderValue::from_str(&to_string(ce)).unwrap());
        let storage_class = config.storage_class.unwrap_or_default();
        let storage_class = HeaderValue::from_str(&to_string(storage_class)).unwrap();
        let metadata = config
            .metadata
            .as_ref()
            .map(|metadata| {
                metadata
                    .iter()
                    .map(make_header)
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_else(|| Ok(vec![]))?;
        let extension = config
            .filename_extension
            .clone()
            .unwrap_or_else(|| config.compression.extension().into());
        let time_format = config
            .filename_time_format
            .clone()
            .unwrap_or_else(|| "%s".into());
        let append_uuid = config.filename_append_uuid.unwrap_or(true);
        Ok(Self {
            acl,
            content_type,
            content_encoding,
            storage_class,
            headers: metadata,
            extension,
            time_format,
            append_uuid,
            compression: config.compression,
            encoder: (transformer, encoder),
        })
    }
}

// Make a header pair from a key-value string pair
fn make_header((name, value): (&String, &String)) -> crate::Result<(HeaderName, HeaderValue)> {
    Ok((
        HeaderName::from_bytes(name.as_bytes())?,
        HeaderValue::from_str(value)?,
    ))
}

#[cfg(test)]
mod tests {
    use futures_util::{future::ready, stream};
    use vector_core::partition::Partitioner;

    use crate::test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        http::{always_200_response, spawn_blackhole_http_server},
    };

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<GcsSinkConfig>();
    }

    #[tokio::test]
    async fn component_spec_compliance() {
        let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

        let context = SinkContext::new_test();

        let tls = TlsSettings::default();
        let client =
            HttpClient::new(tls, context.proxy()).expect("should not fail to create HTTP client");

        let config = default_config(StandardEncodings::Json);
        let sink = config
            .build_sink(client, mock_endpoint.to_string(), None, context)
            .expect("failed to build sink");

        let event = Event::from("simple message");
        run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;
    }

    #[test]
    fn gcs_encode_event_apply_rules() {
        crate::test_util::trace_init();

        let message = "hello world".to_string();
        let mut event = Event::from(message);
        event.as_mut_log().insert("key", "value");

        let sink_config = GcsSinkConfig {
            key_prefix: Some("key: {{ key }}".into()),
            ..default_config(StandardEncodings::Text)
        };
        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&event)
            .expect("key wasn't provided");

        assert_eq!(key, "key: value");
    }

    fn request_settings(sink_config: &GcsSinkConfig) -> RequestSettings {
        RequestSettings::new(sink_config).expect("Could not create request settings")
    }

    fn build_request(extension: Option<&str>, uuid: bool, compression: Compression) -> GcsRequest {
        let log = Event::new_empty_log();
        let sink_config = GcsSinkConfig {
            key_prefix: Some("key/".into()),
            filename_time_format: Some("date".into()),
            filename_extension: extension.map(Into::into),
            filename_append_uuid: Some(uuid),
            compression,
            ..default_config(StandardEncodings::Ndjson)
        };
        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&log)
            .expect("key wasn't provided");
        let request_settings = request_settings(&sink_config);
        let (metadata, _events) = request_settings.split_input((key, vec![log]));
        request_settings.build_request(metadata, EncodeResult::uncompressed(Bytes::new()))
    }

    #[test]
    fn gcs_build_request() {
        let req = build_request(Some("ext"), false, Compression::None);
        assert_eq!(req.key, "key/date.ext".to_string());

        let req = build_request(None, false, Compression::None);
        assert_eq!(req.key, "key/date.log".to_string());

        let req = build_request(None, false, Compression::gzip_default());
        assert_eq!(req.key, "key/date.log.gz".to_string());

        let req = build_request(None, true, Compression::gzip_default());
        assert_ne!(req.key, "key/date.log.gz".to_string());
    }
}
