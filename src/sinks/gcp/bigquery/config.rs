use crate::codecs::encoding::ProtobufSerializerConfig;
use futures::FutureExt;
use indoc::indoc;
use tonic::transport::Channel;
use tower::ServiceBuilder;
use vector_config::configurable_component;

use super::proto::google::cloud::bigquery::storage::v1 as proto;
use super::request_builder::{BigqueryRequestBuilder, MAX_BATCH_PAYLOAD_SIZE};
use super::service::{AuthInterceptor, BigqueryRetryLogic, BigqueryService};
use super::sink::BigquerySink;
use crate::config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext};
use crate::gcp::{BIGQUERY_STORAGE_URL, GcpAuthConfig, GcpAuthenticator, Scope};
use crate::sinks::util::service::ServiceBuilderExt;
use crate::sinks::util::{BatchConfig, SinkBatchSettings, TowerRequestConfig};
use crate::sinks::{Healthcheck, VectorSink};

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
    /// by the [`GCP BigQuery`][bigquery_api] API are used.
    ///
    /// The trailing slash `/` must not be included.
    ///
    /// [bigquery_api]: https://cloud.google.com/bigquery/docs/reference/rest
    #[serde(default = "default_endpoint")]
    #[configurable(metadata(docs::examples = "https://bigquerystorage.googleapis.com:443"))]
    pub endpoint: String,

    #[serde(default, flatten)]
    pub auth: GcpAuthConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<BigqueryDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    // we only support protobuf encoding because that's what the API uses (gRPC)
    #[configurable(derived)]
    encoding: ProtobufSerializerConfig,

    #[configurable(derived)]
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
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            project = "my-project"
            dataset = "my-dataset"
            table = "my-table"
            encoding.protobuf.desc_file = "/etc/vector/proto.desc"
            encoding.protobuf.message_type = "BigqueryMessage"
        "#})
        .unwrap()
    }
}

/// Create a future that calls GetWriteStream to verify connectivity and auth.
async fn healthcheck_future(
    channel: Channel,
    auth: GcpAuthenticator,
    write_stream: String,
) -> crate::Result<()> {
    let mut client = proto::big_query_write_client::BigQueryWriteClient::with_interceptor(
        channel,
        AuthInterceptor { auth },
    );
    client
        .get_write_stream(proto::GetWriteStreamRequest {
            name: write_stream,
            view: proto::WriteStreamView::Full as i32,
        })
        .await?;
    Ok(())
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_bigquery")]
impl SinkConfig for BigqueryConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        // `cx.proxy` doesn't apply well to tonic gRPC connections,
        // so we don't use it when building the sink.

        // Configure auth and make sure it's constantly up-to-date
        let auth = self.auth.build(Scope::BigQueryInsertdata).await?;
        if matches!(auth, GcpAuthenticator::ApiKey(_)) {
            return Err(
                "GCP BigQuery sink does not support API key authentication. Use service account credentials or workload identity instead".into(),
            );
        }
        auth.spawn_regenerate_token();

        // Create the gRPC channel; tonic channels are cheap to clone and share the underlying connection.
        let channel = Channel::builder(self.endpoint.parse()?)
            .connect_timeout(std::time::Duration::from_secs(10))
            .connect()
            .await?;

        // Kick off the healthcheck
        let healthcheck: Healthcheck = if cx.healthcheck.enabled {
            healthcheck_future(channel.clone(), auth.clone(), self.get_write_stream()).boxed()
        } else {
            Box::pin(async move { Ok(()) })
        };

        let request_settings = self.request.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_settings, BigqueryRetryLogic)
            .service(BigqueryService::with_auth(channel, auth));

        let batcher_settings = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_BATCH_PAYLOAD_SIZE)?
            .into_batcher_settings()?;

        let protobuf_serializer = self.encoding.build()?;
        let write_stream = self.get_write_stream();
        let request_builder = BigqueryRequestBuilder::new(protobuf_serializer, write_stream)?;

        let sink = BigquerySink {
            service,
            batcher_settings,
            request_builder,
        };
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
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
