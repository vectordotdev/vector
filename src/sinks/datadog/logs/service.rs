use crate::sinks::datadog::logs::config::Encoding;
use crate::sinks::datadog::ApiKey;
use crate::sinks::util::buffer::GZIP_FAST;
use crate::sinks::util::encoding::EncodingConfigWithDefault;
use crate::sinks::util::encoding::EncodingConfiguration;
use crate::sinks::util::http::HttpSink;
use crate::sinks::util::Compression;
use crate::sinks::util::{BoxedRawValue, PartitionInnerBuffer};
use crate::{config::log_schema, internal_events::DatadogLogEventProcessed};
use flate2::write::GzEncoder;
use http::Request;
use http::Uri;
use serde_json::json;
use std::io::Write;
use std::sync::Arc;
use vector_core::config::LogSchema;
use vector_core::event::Event;

#[derive(Debug, Default)]
pub struct ServiceBuilder {
    uri: Option<Uri>,
    default_api_key: Option<ApiKey>,
    compression: Compression,
    encoding: Option<EncodingConfigWithDefault<Encoding>>,
    log_schema_message_key: Option<&'static str>,
    log_schema_timestamp_key: Option<&'static str>,
    log_schema_host_key: Option<&'static str>,
}

impl ServiceBuilder {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub(crate) fn uri(mut self, uri: Uri) -> Self {
        self.uri = Some(uri);
        self
    }

    pub(crate) fn default_api_key(mut self, api_key: ApiKey) -> Self {
        self.default_api_key = Some(api_key);
        self
    }

    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub(crate) fn compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }

    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub(crate) fn encoding(mut self, encoding: EncodingConfigWithDefault<Encoding>) -> Self {
        self.encoding = Some(encoding);
        self
    }

    pub(crate) fn log_schema(mut self, log_schema: &'static LogSchema) -> Self {
        self.log_schema_host_key = Some(log_schema.host_key());
        self.log_schema_message_key = Some(log_schema.message_key());
        self.log_schema_timestamp_key = Some(log_schema.timestamp_key());
        self
    }

    pub(crate) fn build(self) -> Service {
        Service {
            uri: self.uri.expect("must set URI"),
            default_api_key: self
                .default_api_key
                .expect("must set a default Datadog API key"),
            compression: self.compression,
            encoding: self.encoding.expect("must set an encoding"),
            log_schema_host_key: self
                .log_schema_host_key
                .unwrap_or_else(|| log_schema().host_key()),
            log_schema_message_key: self
                .log_schema_message_key
                .unwrap_or_else(|| log_schema().message_key()),
            log_schema_timestamp_key: self
                .log_schema_timestamp_key
                .unwrap_or_else(|| log_schema().timestamp_key()),
        }
    }
}

#[derive(Clone)]
pub struct Service {
    uri: Uri,
    default_api_key: ApiKey,
    compression: Compression,
    encoding: EncodingConfigWithDefault<Encoding>,
    log_schema_message_key: &'static str,
    log_schema_timestamp_key: &'static str,
    log_schema_host_key: &'static str,
}

impl Service {
    pub(crate) fn builder() -> ServiceBuilder {
        ServiceBuilder::default()
    }
}

#[async_trait::async_trait]
impl HttpSink for Service {
    type Input = PartitionInnerBuffer<serde_json::Value, ApiKey>;
    type Output = PartitionInnerBuffer<Vec<BoxedRawValue>, ApiKey>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        let log = event.as_mut_log();
        log.rename_key_flat(self.log_schema_message_key, "message");
        log.rename_key_flat(self.log_schema_timestamp_key, "date");
        log.rename_key_flat(self.log_schema_host_key, "host");
        self.encoding.apply_rules(&mut event);

        let (fields, metadata) = event.into_log().into_parts();
        let json_event = json!(fields);
        let api_key = metadata
            .datadog_api_key()
            .as_ref()
            .unwrap_or(&self.default_api_key);

        Some(PartitionInnerBuffer::new(json_event, Arc::clone(api_key)))
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
        let (events, api_key) = events.into_parts();

        let body: Vec<u8> = serde_json::to_vec(&events)?;
        // check the number of events to ignore health-check requests
        if !events.is_empty() {
            emit!(DatadogLogEventProcessed {
                byte_size: body.len(),
                count: events.len(),
            });
        }

        let request = Request::post(self.uri.clone())
            .header("Content-Type", "application/json")
            .header("DD-API-KEY", &api_key[..]);

        let (request, encoded_body) = match self.compression {
            Compression::None => (request, body),
            Compression::Gzip(level) => {
                let level = level.unwrap_or(GZIP_FAST);
                let mut encoder = GzEncoder::new(
                    Vec::with_capacity(body.len()),
                    flate2::Compression::new(level as u32),
                );

                encoder.write_all(&body)?;
                (
                    request.header("Content-Encoding", "gzip"),
                    encoder.finish()?,
                )
            }
        };

        request
            .header("Content-Length", encoded_body.len())
            .body(encoded_body)
            .map_err(Into::into)
    }
}
