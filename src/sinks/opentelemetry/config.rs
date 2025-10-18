//! Configuration for the OpenTelemetry sink with custom partitioning strategies.

use std::collections::BTreeMap;

use http::StatusCode;
use hyper::Body;
use vector_config::configurable_component;
use vector_lib::codecs::encoding::{Framer, Serializer};

use super::sink::OpenTelemetrySink;
use crate::{
    codecs::EncodingConfigWithFraming,
    http::{Auth, HttpClient, MaybeAuth},
    sinks::{
        http::{
            config::{validate_headers, validate_payload_wrapper, HttpMethod, HttpSinkConfig},
            encoder::HttpEncoder,
            request_builder::HttpRequestBuilder,
            service::{HttpService, HttpSinkRequestBuilder},
        },
        prelude::*,
        util::{
            http::{http_response_retry_logic, OrderedHeaderName, RequestConfig},
            RealtimeSizeBasedDefaultBatchSettings, UriSerde,
        },
    },
};

/// Partitioning strategy for OpenTelemetry events.
///
/// This determines how events are grouped into batches for transmission.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PartitionStrategy {
    /// Partition by URI and headers.
    ///
    /// This is the legacy behavior that partitions events based on the
    /// templated URI and headers. This can lead to poor batching for OTLP
    /// data where all events typically go to the same endpoint.
    #[default]
    UriHeaders,

    /// Partition by InstrumentationScope.
    ///
    /// Groups events by their OTLP InstrumentationScope (name + version).
    /// This allows multiple ResourceLogs/ResourceMetrics/ResourceSpans with
    /// the same instrumentation scope to be batched together efficiently,
    /// improving throughput and reducing request overhead.
    ///
    /// This is the recommended strategy for OTLP data.
    InstrumentationScope,
}

/// Configuration options specific to the OpenTelemetry sink.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct OpenTelemetryOptions {
    /// The partitioning strategy for batching events.
    ///
    /// This determines how events are grouped into batches before transmission.
    /// Using `instrumentation_scope` can significantly improve batching efficiency
    /// for OTLP data.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "instrumentation_scope"))]
    pub partition_strategy: PartitionStrategy,
}

impl Default for OpenTelemetryOptions {
    fn default() -> Self {
        Self {
            partition_strategy: PartitionStrategy::InstrumentationScope,
        }
    }
}

/// Build an OpenTelemetry sink from HTTP sink configuration with custom partitioning.
pub async fn build_opentelemetry_sink(
    http_config: &HttpSinkConfig,
    opentelemetry_options: &OpenTelemetryOptions,
    cx: SinkContext,
) -> crate::Result<(VectorSink, Healthcheck)> {
    let batch_settings = http_config.batch.validate()?.into_batcher_settings()?;

    let encoder = http_config.build_encoder()?;
    let transformer = http_config.encoding.transformer();

    let mut request = http_config.request.clone();
    request.add_old_option(http_config.headers.clone());

    validate_headers(&request.headers, http_config.auth.is_some())?;
    let (static_headers, template_headers) = request.split_headers();

    let (payload_prefix, payload_suffix) = validate_payload_wrapper(
        &http_config.payload_prefix,
        &http_config.payload_suffix,
        &encoder,
    )?;

    let client = build_http_client(http_config, &cx)?;

    let healthcheck = match cx.healthcheck.uri {
        Some(healthcheck_uri) => {
            healthcheck(healthcheck_uri, http_config.auth.clone(), client.clone()).boxed()
        }
        None => future::ok(()).boxed(),
    };

    let content_type = determine_content_type(&encoder);

    let request_builder = HttpRequestBuilder {
        encoder: HttpEncoder::new(encoder, transformer, payload_prefix, payload_suffix),
        compression: http_config.compression,
    };

    let content_encoding = http_config.compression.is_compressed().then(|| {
        http_config
            .compression
            .content_encoding()
            .expect("Encoding should be specified for compression.")
            .to_string()
    });

    let converted_static_headers = convert_headers(static_headers)?;

    let http_sink_request_builder = HttpSinkRequestBuilder::new(
        http_config.method,
        http_config.auth.clone(),
        converted_static_headers,
        content_type,
        content_encoding,
    );

    let service = build_service(http_config, client, http_sink_request_builder).await?;

    let request_limits = http_config.request.tower.into_settings();

    let service = ServiceBuilder::new()
        .settings(request_limits, http_response_retry_logic())
        .service(service);

    let sink = OpenTelemetrySink::new(
        service,
        http_config.uri.clone(),
        template_headers,
        batch_settings,
        request_builder,
        opentelemetry_options.partition_strategy,
    );

    Ok((VectorSink::from_event_streamsink(sink), healthcheck))
}

fn build_http_client(config: &HttpSinkConfig, cx: &SinkContext) -> crate::Result<HttpClient> {
    let tls = TlsSettings::from_options(config.tls.as_ref())?;
    Ok(HttpClient::new(tls, cx.proxy())?)
}

async fn healthcheck(uri: UriSerde, auth: Option<Auth>, client: HttpClient) -> crate::Result<()> {
    let auth = auth.choose_one(&uri.auth)?;
    let uri = uri.with_default_parts();
    let mut request = http::Request::head(&uri.uri)
        .body(Body::empty())
        .unwrap();

    if let Some(auth) = auth {
        auth.apply(&mut request);
    }

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

fn determine_content_type(encoder: &Encoder<Framer>) -> Option<String> {
    use Framer::*;
    use Serializer::*;
    use vector_lib::codecs::CharacterDelimitedEncoder;

    match (encoder.serializer(), encoder.framer()) {
        (RawMessage(_) | Text(_), _) => Some("text/plain".to_owned()),
        (Json(_), NewlineDelimited(_)) => Some("application/x-ndjson".to_owned()),
        (Json(_), CharacterDelimited(CharacterDelimitedEncoder { delimiter: b',' })) => {
            Some("application/json".to_owned())
        }
        #[cfg(feature = "codecs-opentelemetry")]
        (Otlp(_), _) => Some("application/x-protobuf".to_owned()),
        _ => None,
    }
}

fn convert_headers(
    static_headers: BTreeMap<String, String>,
) -> crate::Result<BTreeMap<OrderedHeaderName, http::HeaderValue>> {
    static_headers
        .into_iter()
        .map(|(name, value)| -> crate::Result<_> {
            let header_name = http::HeaderName::from_bytes(name.as_bytes())
                .map(OrderedHeaderName::from)?;
            let header_value = http::HeaderValue::try_from(value)?;
            Ok((header_name, header_value))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
}

#[cfg(feature = "aws-core")]
async fn build_service(
    config: &HttpSinkConfig,
    client: HttpClient,
    http_sink_request_builder: HttpSinkRequestBuilder,
) -> crate::Result<impl Service<crate::sinks::util::http::HttpRequest<super::sink::PartitionKey>, Response = http::Response<bytes::Bytes>, Error = crate::Error>>
{
    use crate::{aws::AwsAuthentication, sinks::util::http::SigV4Config};
    use aws_config::meta::region::ProvideRegion;
    use aws_types::region::Region;
    use vector_lib::config::proxy::ProxyConfig;

    match &config.auth {
        Some(Auth::Aws { auth, service }) => {
            let default_region = crate::aws::region_provider(&ProxyConfig::default(), None)?
                .region()
                .await;
            let region = (match &auth {
                AwsAuthentication::AccessKey { region, .. } => region.clone(),
                AwsAuthentication::File { .. } => None,
                AwsAuthentication::Role { region, .. } => region.clone(),
                AwsAuthentication::Default { region, .. } => region.clone(),
            })
            .map_or(default_region, |r| Some(Region::new(r.to_string())))
            .expect("Region must be specified");

            Ok(HttpService::new_with_sig_v4(
                client,
                http_sink_request_builder,
                SigV4Config {
                    shared_credentials_provider: auth
                        .credentials_provider(region.clone(), &ProxyConfig::default(), None)
                        .await?,
                    region: region.clone(),
                    service: service.clone(),
                },
            ))
        }
        _ => Ok(HttpService::new(client, http_sink_request_builder)),
    }
}

#[cfg(not(feature = "aws-core"))]
async fn build_service(
    _config: &HttpSinkConfig,
    client: HttpClient,
    http_sink_request_builder: HttpSinkRequestBuilder,
) -> crate::Result<impl Service<crate::sinks::util::http::HttpRequest<super::sink::PartitionKey>, Response = http::Response<bytes::Bytes>, Error = crate::Error>>
{
    Ok(HttpService::new(client, http_sink_request_builder))
}
