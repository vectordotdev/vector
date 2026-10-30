use crate::codecs::encoding::ProtobufSerializerConfig;
use futures::FutureExt;
use indoc::indoc;

use http_1::uri::Scheme;
use hyper_util::{client::legacy::Builder, rt::TokioExecutor};
use tower::ServiceBuilder;
use vector_config::configurable_component;
use vector_http::{GrpcTransport, Http2Client};

use super::proto::google::cloud::bigquery::storage::v1 as proto;
use super::request_builder::{BigqueryRequestBuilder, MAX_BATCH_PAYLOAD_SIZE};
use super::service::{
    BigQueryWriteClient, BigqueryRetryLogic, BigqueryService, RequestAuth, Transport,
};
use super::sink::BigquerySink;
use crate::config::{
    AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
};
use crate::gcp::{BIGQUERY_STORAGE_URL, GcpAuthConfig, GcpAuthenticator, Scope};
use crate::sinks::util::service::ServiceBuilderExt;
use crate::sinks::util::{BatchConfig, SinkBatchSettings, TowerRequestConfig};
use crate::sinks::{Healthcheck, VectorSink};
use crate::tls::{TlsConfig, TlsSettings};
use vector_lib::stream::BatcherSettings;

fn default_endpoint() -> String {
    BIGQUERY_STORAGE_URL.to_string()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BigqueryDefaultBatchSettings;

impl SinkBatchSettings for BigqueryDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(50_000); // Arbitrary limit, there's no hard limit in BigQuery.
    const MAX_BYTES: Option<usize> = Some(MAX_BATCH_PAYLOAD_SIZE);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `gcp_bigquery` sink.
#[configurable_component(sink("gcp_bigquery", "Store events in BigQuery."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BigqueryConfig {
    /// The project name to which to publish events.
    #[configurable(metadata(docs::examples = "vector-123456"))]
    pub project: String,

    /// The dataset within the project to which to publish events.
    #[configurable(metadata(docs::examples = "this-is-a-dataset"))]
    pub dataset: String,

    /// The table within the dataset to which to publish events.
    #[configurable(metadata(docs::examples = "this-is-a-table"))]
    pub table: String,

    /// The endpoint to which to publish events.
    ///
    /// The scheme (`http` or `https`) must be specified. No path should be included since the paths defined
    /// by the [BigQuery Storage Write API][bigquery_write_api] are used.
    ///
    /// The trailing slash `/` must not be included.
    ///
    /// [bigquery_write_api]: https://cloud.google.com/bigquery/docs/write-api
    #[serde(default = "default_endpoint")]
    #[configurable(metadata(docs::examples = "https://bigquerystorage.googleapis.com:443"))]
    pub endpoint: String,

    /// Path to a [service account][gcp_service_account_credentials] credentials JSON file.
    ///
    /// If unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
    /// filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
    /// running on.
    ///
    /// API key authentication is not supported by the BigQuery Storage Write API.
    ///
    /// [gcp_service_account_credentials]: https://cloud.google.com/docs/authentication/production#manually
    pub credentials_path: Option<String>,

    /// Skip all authentication handling. For use with integration tests only.
    #[serde(default, skip_serializing)]
    #[configurable(metadata(docs::hidden))]
    pub skip_authentication: bool,

    #[serde(default)]
    pub batch: BatchConfig<BigqueryDefaultBatchSettings>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    /// Encoding configuration.
    ///
    /// Only Protobuf is supported, because the Storage Write API ingests rows as serialized Protobuf messages.
    encoding: ProtobufSerializerConfig,

    pub tls: Option<TlsConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

impl BigqueryConfig {
    fn get_write_stream(&self) -> String {
        // TODO: support non-default streams
        // https://cloud.google.com/bigquery/docs/write-api#application-created_streams
        format!(
            "projects/{}/datasets/{}/tables/{}/streams/_default",
            self.project, self.dataset, self.table
        )
    }
}

impl GenerateConfig for BigqueryConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc! {r"
            project: my-project
            dataset: my-dataset
            table: my-table
            encoding:
              protobuf:
                desc_file: /etc/vector/proto.desc
                message_type: BigqueryMessage
        "})
        .unwrap()
    }
}

/// Create a future that calls `GetWriteStream` to verify connectivity and auth.
async fn healthcheck_future(
    transport: Transport,
    origin: http_1::Uri,
    auth: GcpAuthenticator,
    write_stream: String,
) -> crate::Result<()> {
    let request = RequestAuth::for_get_write_stream(auth, &write_stream)?.request(
        proto::GetWriteStreamRequest {
            name: write_stream,
            view: proto::WriteStreamView::Basic as i32,
        },
    )?;
    BigQueryWriteClient::with_origin(transport, origin)
        .get_write_stream(request)
        .await?;
    Ok(())
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_bigquery")]
impl SinkConfig for BigqueryConfig {
    fn input(&self) -> Input {
        Input::new(self.encoding.input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedBigquery {
    endpoint: http_1::Uri,
    write_stream: String,
    batcher_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for BigqueryConfig {
    type Validated = ValidatedBigquery;

    fn validate(&self) -> crate::Result<ValidatedBigquery> {
        let endpoint: http_1::Uri = self.endpoint.parse()?;
        let scheme = endpoint.scheme();
        if scheme != Some(&Scheme::HTTP) && scheme != Some(&Scheme::HTTPS) {
            return Err(format!(
                "`endpoint` must use the `http` or `https` scheme, got `{}`",
                self.endpoint
            )
            .into());
        }
        if endpoint.host().is_none() {
            return Err(format!("`endpoint` must include a host, got `{}`", self.endpoint).into());
        }
        if self.tls.is_some() && scheme != Some(&Scheme::HTTPS) {
            return Err("`tls` options require an `https` endpoint".into());
        }
        let batcher_settings = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_BATCH_PAYLOAD_SIZE)?
            .into_batcher_settings()?;

        Ok(ValidatedBigquery {
            endpoint,
            write_stream: self.get_write_stream(),
            batcher_settings,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedBigquery,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        // Configure auth and make sure it's constantly up-to-date
        let auth = GcpAuthConfig {
            api_key: None,
            credentials_path: self.credentials_path.clone(),
            skip_authentication: self.skip_authentication,
        }
        .build(Scope::BigQueryInsertdata)
        .await?;
        auth.spawn_regenerate_token();

        let mut tls_config = self.tls.clone().unwrap_or_default();
        tls_config
            .alpn_protocols
            // Google's frontends negotiate gRPC over http2.
            .get_or_insert_with(|| vec!["h2".to_owned()]);

        let tls = TlsSettings::from_options(Some(&tls_config))?.with_system_roots();
        let mut builder = Builder::new(TokioExecutor::new());
        builder.http2_max_header_list_size(vector_common::constants::GRPC_MAX_HEADER_LIST_SIZE);
        let transport = GrpcTransport(Http2Client::new(tls.into(), &cx.proxy, builder)?);

        let healthcheck: Healthcheck = if cx.healthcheck.enabled {
            healthcheck_future(
                transport.clone(),
                validated.endpoint.clone(),
                auth.clone(),
                validated.write_stream.clone(),
            )
            .boxed()
        } else {
            Box::pin(async move { Ok(()) })
        };

        let request_settings = self.request.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_settings, BigqueryRetryLogic)
            .service(BigqueryService::new(
                transport,
                validated.endpoint.clone(),
                auth,
                &validated.write_stream,
            )?);

        let protobuf_serializer = self.encoding.build()?;
        let request_builder =
            BigqueryRequestBuilder::new(protobuf_serializer, validated.write_stream.clone())?;

        let sink = BigquerySink {
            service,
            batcher_settings: validated.batcher_settings,
            request_builder,
        };
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

#[cfg(test)]
mod test {
    use super::BigqueryConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<BigqueryConfig>();
    }
}
