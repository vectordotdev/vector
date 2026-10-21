use crate::codecs::encoding::ProtobufSerializerConfig;
use futures::FutureExt;
use indoc::indoc;
use std::path::PathBuf;

use http::uri::Scheme;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tower::ServiceBuilder;
use vector_config::configurable_component;

use super::proto::google::cloud::bigquery::storage::v1 as proto;
use super::request_builder::{BigqueryRequestBuilder, MAX_BATCH_PAYLOAD_SIZE};
use super::service::{AuthInterceptor, BigqueryRetryLogic, BigqueryService};
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

    #[serde(default)]
    pub tls: Option<BigqueryTlsConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

/// TLS configuration.
///
/// TLS is used whenever `endpoint` uses the `https` scheme, verifying the server against the system's
/// trusted root certificates. These options customize it.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct BigqueryTlsConfig {
    /// Absolute path to an additional CA certificate file, trusted in addition to the system's root certificates.
    ///
    /// The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
    #[configurable(metadata(docs::examples = "/path/to/certificate_authority.crt"))]
    #[configurable(metadata(docs::human_name = "CA File Path"))]
    pub ca_file: Option<PathBuf>,

    /// Absolute path to a certificate file used to identify this client.
    ///
    /// The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
    /// an inline string in PEM format.
    ///
    /// If this is set _and_ is not a PKCS#12 archive, `key_file` must also be set.
    #[configurable(metadata(docs::examples = "/path/to/host_certificate.crt"))]
    #[configurable(metadata(docs::human_name = "Certificate File Path"))]
    pub crt_file: Option<PathBuf>,

    /// Absolute path to a private key file used to identify this client.
    ///
    /// The key must be in DER or PEM (PKCS#8) format. Additionally, the key can be provided as an inline string in PEM format.
    #[configurable(metadata(docs::examples = "/path/to/host_certificate.key"))]
    #[configurable(metadata(docs::human_name = "Key File Path"))]
    pub key_file: Option<PathBuf>,

    /// Passphrase used to unlock the encrypted key file.
    ///
    /// This has no effect unless `key_file` is set.
    #[configurable(metadata(docs::examples = "${KEY_PASS_ENV_VAR}"))]
    #[configurable(metadata(docs::examples = "PassWord1"))]
    #[configurable(metadata(docs::human_name = "Key File Password"))]
    pub key_pass: Option<String>,

    /// Server name to use for Server Name Indication (SNI) and to verify the server's certificate.
    ///
    /// Defaults to the host of `endpoint`.
    #[configurable(metadata(docs::examples = "bigquerystorage.googleapis.com"))]
    #[configurable(metadata(docs::human_name = "Server Name"))]
    pub server_name: Option<String>,
}

impl BigqueryTlsConfig {
    /// Loads the certificate files and builds the tonic TLS configuration.
    fn client_tls_config(&self) -> crate::Result<ClientTlsConfig> {
        let settings = TlsSettings::from_options(Some(&TlsConfig {
            ca_file: self.ca_file.clone(),
            crt_file: self.crt_file.clone(),
            key_file: self.key_file.clone(),
            key_pass: self.key_pass.clone(),
            ..TlsConfig::default()
        }))?;

        let mut config = ClientTlsConfig::new();
        if let Some(server_name) = &self.server_name {
            config = config.domain_name(server_name);
        }

        let authorities: Vec<u8> = settings.authorities_pem().flatten().collect();
        if !authorities.is_empty() {
            config = config.ca_certificate(Certificate::from_pem(authorities));
        }

        if let Some((cert, key)) = settings.identity_pem() {
            config = config.identity(Identity::from_pem(cert, key));
        }
        Ok(config)
    }
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
    channel: Channel,
    auth: GcpAuthenticator,
    write_stream: String,
) -> crate::Result<()> {
    let mut client = proto::big_query_write_client::BigQueryWriteClient::with_interceptor(
        channel,
        AuthInterceptor::for_get_write_stream(auth, &write_stream)?,
    );
    client
        .get_write_stream(proto::GetWriteStreamRequest {
            name: write_stream,
            view: proto::WriteStreamView::Basic as i32,
        })
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
    endpoint: http::Uri,
    write_stream: String,
    batcher_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for BigqueryConfig {
    type Validated = ValidatedBigquery;

    fn validate(&self) -> crate::Result<ValidatedBigquery> {
        let endpoint: http::Uri = self.endpoint.parse()?;
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
        // `cx.proxy` doesn't apply well to tonic gRPC connections,
        // so we don't use it when building the sink.

        // Configure auth and make sure it's constantly up-to-date
        let auth = GcpAuthConfig {
            api_key: None,
            credentials_path: self.credentials_path.clone(),
            skip_authentication: self.skip_authentication,
        }
        .build(Scope::BigQueryInsertdata)
        .await?;
        auth.spawn_regenerate_token();

        let mut endpoint = Channel::builder(validated.endpoint.clone())
            .connect_timeout(std::time::Duration::from_secs(10));
        if validated.endpoint.scheme() == Some(&Scheme::HTTPS) {
            let tls = self.tls.clone().unwrap_or_default().client_tls_config()?;
            endpoint = endpoint.tls_config(tls)?;
        }

        // Connect lazily so an unreachable endpoint at startup surfaces as a retriable error on the
        // first request instead of failing the sink build.
        let channel = endpoint.connect_lazy();

        // Kick off the healthcheck
        let healthcheck: Healthcheck = if cx.healthcheck.enabled {
            healthcheck_future(
                channel.clone(),
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
                channel,
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
