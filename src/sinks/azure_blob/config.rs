use std::fs::File;
use std::io::Read;
use std::sync::Arc;

use azure_core::{
    Error,
    credentials::TokenCredential,
    error::ErrorKind,
    http::{StatusCode, Url},
};
use azure_storage_blob::{BlobContainerClient, BlobContainerClientOptions};

use bytes::Bytes;
use futures::FutureExt;
use snafu::Snafu;
use tower::ServiceBuilder;
use vector_lib::{
    codecs::{JsonSerializerConfig, NewlineDelimitedEncoderConfig, encoding::Framer},
    configurable::configurable_component,
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    sensitive_string::SensitiveString,
    stream::DriverResponse,
};

use super::request_builder::AzureBlobRequestOptions;
use crate::{
    Result,
    codecs::{Encoder, EncodingConfigWithFraming, SinkType},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    event::{EventFinalizers, EventStatus, Finalizable},
    sinks::{
        Healthcheck, VectorSink,
        azure_blob::{service::AzureBlobService, sink::AzureBlobSink},
        azure_common::{
            config::AzureAuthentication,
            config::AzureBlobTlsConfig,
            connection_string::{Auth, ParsedConnectionString},
            shared_key_policy::SharedKeyAuthorizationPolicy,
        },
        util::{
            BatchConfig, BulkSizeBasedDefaultBatchSettings, Compression, ServiceBuilderExt,
            TowerRequestConfig, partitioner::KeyPartitioner, retries::RetryLogic,
            service::TowerRequestConfigDefaults,
        },
    },
    template::{ConfinementConfig, Template},
};

#[derive(Clone, Copy, Debug)]
pub struct AzureBlobTowerRequestConfigDefaults;

impl TowerRequestConfigDefaults for AzureBlobTowerRequestConfigDefaults {
    const RATE_LIMIT_NUM: u64 = 250;
}

/// Configuration for the `azure_blob` sink.
#[configurable_component(sink(
    "azure_blob",
    "Store your observability data in Azure Blob Storage."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AzureBlobSinkConfig {
    #[configurable(derived)]
    #[serde(default)]
    pub auth: Option<AzureAuthentication>,

    /// The Azure Blob Storage Account connection string.
    ///
    /// Authentication with an access key or shared access signature (SAS)
    /// are supported authentication methods. If using a non-account SAS,
    /// healthchecks will fail and will need to be disabled by setting
    /// `healthcheck.enabled` to `false` for this sink
    ///
    /// When generating an account SAS, the following are the minimum required option
    /// settings for Vector to access blob storage and pass a health check.
    /// | Option                 | Value              |
    /// | ---------------------- | ------------------ |
    /// | Allowed services       | Blob               |
    /// | Allowed resource types | Container & Object |
    /// | Allowed permissions    | Read & Create      |
    #[configurable(metadata(
        docs::warnings = "Access keys and SAS tokens can be used to gain unauthorized access to Azure Blob Storage \
        resources. Numerous security breaches have occurred due to leaked connection strings. It is important to keep \
        connection strings secure and not expose them in logs, error messages, or version control systems."
    ))]
    #[configurable(metadata(
        docs::examples = "DefaultEndpointsProtocol=https;AccountName=mylogstorage;AccountKey=storageaccountkeybase64encoded;EndpointSuffix=core.windows.net"
    ))]
    #[configurable(metadata(
        docs::examples = "BlobEndpoint=https://mylogstorage.blob.core.windows.net/;SharedAccessSignature=generatedsastoken"
    ))]
    #[configurable(metadata(docs::examples = "AccountName=mylogstorage"))]
    pub connection_string: Option<SensitiveString>,

    /// The Azure Blob Storage Account name.
    ///
    /// If provided, this will be used instead of the `connection_string`.
    /// This is useful for authenticating with an Azure credential.
    #[configurable(metadata(docs::examples = "mylogstorage"))]
    pub(super) account_name: Option<String>,

    /// The Azure Blob Storage endpoint.
    ///
    /// If provided, this will be used instead of the `connection_string`.
    /// This is useful for authenticating with an Azure credential.
    #[configurable(metadata(docs::examples = "https://mylogstorage.blob.core.windows.net/"))]
    pub(super) blob_endpoint: Option<String>,

    /// The Azure Blob Storage Account container name.
    #[configurable(metadata(docs::examples = "my-logs"))]
    pub(super) container_name: String,

    /// A prefix to apply to all blob keys.
    ///
    /// Prefixes are useful for partitioning objects, such as by creating a blob key that
    /// stores blobs under a particular directory. If using a prefix for this purpose, it must end
    /// in `/` to act as a directory path. A trailing `/` is **not** automatically added.
    #[configurable(metadata(docs::examples = "date/%F/hour/%H/"))]
    #[configurable(metadata(docs::examples = "year=%Y/month=%m/day=%d/"))]
    #[configurable(metadata(
        docs::examples = "kubernetes/{{ metadata.cluster }}/{{ metadata.application_name }}/"
    ))]
    #[serde(default = "default_blob_prefix")]
    pub blob_prefix: Template,

    /// The timestamp format for the time component of the blob key.
    ///
    /// By default, blob keys are appended with a timestamp that reflects when the blob are sent to
    /// Azure Blob Storage, such that the resulting blob key is functionally equivalent to joining
    /// the blob prefix with the formatted timestamp, such as `date=2022-07-18/1658176486`.
    ///
    /// This would represent a `blob_prefix` set to `date=%F/` and the timestamp of Mon Jul 18 2022
    /// 20:34:44 GMT+0000, with the `filename_time_format` being set to `%s`, which renders
    /// timestamps in seconds since the Unix epoch.
    ///
    /// Supports the common [`strftime`][chrono_strftime_specifiers] specifiers found in most
    /// languages.
    ///
    /// When set to an empty string, no timestamp is appended to the blob prefix.
    ///
    /// [chrono_strftime_specifiers]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
    #[configurable(metadata(docs::syntax_override = "strftime"))]
    pub blob_time_format: Option<String>,

    /// Whether or not to append a UUID v4 token to the end of the blob key.
    ///
    /// The UUID is appended to the timestamp portion of the object key, such that if the blob key
    /// generated is `date=2022-07-18/1658176486`, setting this field to `true` results
    /// in an blob key that looks like
    /// `date=2022-07-18/1658176486-30f6652c-71da-4f9f-800d-a1189c47c547`.
    ///
    /// This ensures there are no name collisions, and can be useful in high-volume workloads where
    /// blob keys must be unique.
    pub blob_append_uuid: Option<bool>,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,

    /// Compression configuration.
    ///
    /// All compression algorithms use the default compression level unless otherwise specified.
    ///
    /// Some cloud storage API clients and browsers handle decompression transparently, so
    /// depending on how they are accessed, files may not always appear to be compressed.
    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<BulkSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig<AzureBlobTowerRequestConfigDefaults>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub(super) acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub tls: Option<AzureBlobTlsConfig>,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,
}

pub fn default_blob_prefix() -> Template {
    Template::try_from(DEFAULT_KEY_PREFIX).unwrap()
}

impl GenerateConfig for AzureBlobSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            auth: None,
            connection_string: Some(String::from("DefaultEndpointsProtocol=https;AccountName=some-account-name;AccountKey=some-account-key;").into()),
            account_name: None,
            blob_endpoint: None,
            container_name: String::from("logs"),
            blob_prefix: default_blob_prefix(),
            blob_time_format: Some(String::from("%s")),
            blob_append_uuid: Some(true),
            encoding: (Some(NewlineDelimitedEncoderConfig::new()), JsonSerializerConfig::default()).into(),
            compression: Compression::gzip_default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            acknowledgements: Default::default(),
            tls: None,
            confinement: ConfinementConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "azure_blob")]
impl SinkConfig for AzureBlobSinkConfig {
    async fn build(&self, cx: SinkContext) -> Result<(VectorSink, Healthcheck)> {
        let connection_string: String = match (
            &self.connection_string,
            &self.account_name,
            &self.blob_endpoint,
        ) {
            (Some(connstr), None, None) => connstr.inner().into(),
            (None, Some(account_name), None) => {
                if self.auth.is_none() {
                    return Err(
                        "`auth` configuration must be provided when using `account_name`".into(),
                    );
                }
                format!("AccountName={}", account_name)
            }
            (None, None, Some(blob_endpoint)) => {
                if self.auth.is_none() {
                    return Err(
                        "`auth` configuration must be provided when using `blob_endpoint`".into(),
                    );
                }
                // BlobEndpoint must always end in a trailing slash
                let blob_endpoint = if blob_endpoint.ends_with('/') {
                    blob_endpoint.clone()
                } else {
                    format!("{}/", blob_endpoint)
                };
                format!("BlobEndpoint={}", blob_endpoint)
            }
            (None, None, None) => {
                return Err("One of `connection_string`, `account_name`, or `blob_endpoint` must be provided".into());
            }
            (Some(_), Some(_), _) => {
                return Err("Cannot provide both `connection_string` and `account_name`".into());
            }
            (Some(_), _, Some(_)) => {
                return Err("Cannot provide both `connection_string` and `blob_endpoint`".into());
            }
            (_, Some(_), Some(_)) => {
                return Err("Cannot provide both `account_name` and `blob_endpoint`".into());
            }
        };

        let client = build_client(
            self.auth.clone(),
            connection_string.clone(),
            self.container_name.clone(),
            cx.proxy(),
            self.tls.clone(),
        )
        .await?;

        let healthcheck = build_healthcheck(self.container_name.clone(), Arc::clone(&client))?;
        let sink = self.build_processor(client)?;
        Ok((sink, healthcheck))
    }

    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

const DEFAULT_KEY_PREFIX: &str = "blob/%F/";
const DEFAULT_FILENAME_TIME_FORMAT: &str = "%s";
const DEFAULT_FILENAME_APPEND_UUID: bool = true;

impl AzureBlobSinkConfig {
    pub fn build_processor(&self, client: Arc<BlobContainerClient>) -> crate::Result<VectorSink> {
        let request_limits = self.request.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_limits, AzureBlobRetryLogic)
            .service(AzureBlobService::new(client));

        // Configure our partitioning/batching.
        let batcher_settings = self.batch.into_batcher_settings()?;

        let blob_time_format = self
            .blob_time_format
            .as_ref()
            .cloned()
            .unwrap_or_else(|| DEFAULT_FILENAME_TIME_FORMAT.into());
        let blob_append_uuid = self
            .blob_append_uuid
            .unwrap_or(DEFAULT_FILENAME_APPEND_UUID);

        let transformer = self.encoding.transformer();
        let (framer, serializer) = self.encoding.build(SinkType::MessageBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let request_options = AzureBlobRequestOptions {
            container_name: self.container_name.clone(),
            blob_time_format,
            blob_append_uuid,
            encoder: (transformer, encoder),
            compression: self.compression,
        };

        let sink = AzureBlobSink::new(
            service,
            request_options,
            self.key_partitioner()?,
            batcher_settings,
        );

        Ok(VectorSink::from_event_streamsink(sink))
    }

    pub fn key_partitioner(&self) -> crate::Result<KeyPartitioner> {
        let tpl = self
            .blob_prefix
            .clone()
            .confine(&self.confinement, Self::NAME, "blob_prefix")?;
        Ok(KeyPartitioner::new(tpl, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::ConfinementConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AzureBlobSinkConfig>();
    }

    #[test]
    fn confinement_rejects_unconfined_blob_prefix() {
        let template = Template::try_from("{{ tenant }}").unwrap();
        let err = template
            .confine(&ConfinementConfig::default(), "azure_blob", "blob_prefix")
            .unwrap_err();
        assert!(
            err.to_string().contains("no literal string prefix"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn confinement_opt_out_allows_unconfined_blob_prefix() {
        let cfg = ConfinementConfig {
            dangerously_allow_unconfined_template_resolution: true,
        };
        let template = Template::try_from("{{ tenant }}").unwrap();
        assert!(template.confine(&cfg, "azure_blob", "blob_prefix").is_ok());
    }

    #[test]
    fn confinement_blocks_dotdot_escape_at_render() {
        use crate::event::Event;
        use vector_lib::event::LogEvent;
        use vrl::event_path;

        let template = Template::try_from("safe/{{ tenant }}/").unwrap();
        let template = template
            .confine(&ConfinementConfig::default(), "azure_blob", "blob_prefix")
            .unwrap();
        let mut event = Event::Log(LogEvent::from("x"));
        event
            .as_mut_log()
            .insert(event_path!("tenant"), "../../escape");
        assert!(template.render_string(&event).is_err());
    }
}

#[derive(Debug, Clone)]
pub struct AzureBlobRequest {
    pub blob_data: Bytes,
    pub content_encoding: Option<&'static str>,
    pub content_type: &'static str,
    pub metadata: AzureBlobMetadata,
    pub request_metadata: RequestMetadata,
}

impl Finalizable for AzureBlobRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

impl MetaDescriptive for AzureBlobRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

#[derive(Clone, Debug)]
pub struct AzureBlobMetadata {
    pub partition_key: String,
    pub count: usize,
    pub finalizers: EventFinalizers,
}

#[derive(Debug, Clone)]
pub struct AzureBlobRetryLogic;

impl RetryLogic for AzureBlobRetryLogic {
    type Error = Error;
    type Request = AzureBlobRequest;
    type Response = AzureBlobResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error.http_status() {
            Some(code) => code.is_server_error() || code == StatusCode::TooManyRequests,
            None => false,
        }
    }
}

#[derive(Debug)]
pub struct AzureBlobResponse {
    pub events_byte_size: GroupedCountByteSize,
    pub byte_size: usize,
}

impl DriverResponse for AzureBlobResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}

#[derive(Debug, Snafu)]
pub enum HealthcheckError {
    #[snafu(display("Invalid connection string specified"))]
    InvalidCredentials,
    #[snafu(display("Container: {:?} not found", container))]
    UnknownContainer { container: String },
    #[snafu(display("Unknown status code: {}", status))]
    Unknown { status: StatusCode },
}

pub fn build_healthcheck(
    container_name: String,
    client: Arc<BlobContainerClient>,
) -> crate::Result<Healthcheck> {
    let healthcheck = async move {
        let resp: crate::Result<()> = match client.get_properties(None).await {
            Ok(_) => Ok(()),
            Err(error) => {
                let code = error.http_status();
                Err(match code {
                    Some(StatusCode::Forbidden) => Box::new(HealthcheckError::InvalidCredentials),
                    Some(StatusCode::NotFound) => Box::new(HealthcheckError::UnknownContainer {
                        container: container_name,
                    }),
                    Some(status) => Box::new(HealthcheckError::Unknown { status }),
                    None => "unknown status code".into(),
                })
            }
        };
        resp
    };

    Ok(healthcheck.boxed())
}

pub async fn build_client(
    auth: Option<AzureAuthentication>,
    connection_string: String,
    container_name: String,
    proxy: &crate::config::ProxyConfig,
    tls: Option<AzureBlobTlsConfig>,
) -> crate::Result<Arc<BlobContainerClient>> {
    // Parse connection string without legacy SDK
    let parsed = ParsedConnectionString::parse(&connection_string)
        .map_err(|e| format!("Invalid connection string: {e}"))?;
    // Compose container URL (SAS appended if present)
    let container_url = parsed
        .container_url(&container_name)
        .map_err(|e| format!("Failed to build container URL: {e}"))?;
    let url = Url::parse(&container_url).map_err(|e| format!("Invalid container URL: {e}"))?;

    let mut credential: Option<Arc<dyn TokenCredential>> = None;

    // Prepare options; attach Shared Key policy if needed
    let mut options = BlobContainerClientOptions::default();
    match (parsed.auth(), &auth) {
        (Auth::None, None) => {
            warn!("No authentication method provided, requests will be anonymous.");
        }
        (Auth::Sas { .. }, None) => {
            info!("Using SAS token authentication.");
        }
        (
            Auth::SharedKey {
                account_name,
                account_key,
            },
            None,
        ) => {
            info!("Using Shared Key authentication.");

            let policy = SharedKeyAuthorizationPolicy::new(
                account_name,
                account_key,
                // Use an Azurite-supported storage service version
                String::from("2025-11-05"),
            )
            .map_err(|e| format!("Failed to create SharedKey policy: {e}"))?;
            options
                .client_options
                .per_call_policies
                .push(Arc::new(policy));
        }
        (Auth::None, Some(AzureAuthentication::Specific(..))) => {
            info!("Using Azure Authentication method.");
            let credential_result: Arc<dyn TokenCredential> =
                auth.unwrap().credential().await.map_err(|e| {
                    Error::with_message(
                        ErrorKind::Credential,
                        format!("Failed to configure Azure Authentication: {e}"),
                    )
                })?;
            credential = Some(credential_result);
        }
        (Auth::Sas { .. }, Some(AzureAuthentication::Specific(..))) => {
            return Err(Box::new(Error::with_message(
                ErrorKind::Credential,
                "Cannot use both SAS token and another Azure Authentication method at the same time",
            )));
        }
        (Auth::SharedKey { .. }, Some(AzureAuthentication::Specific(..))) => {
            return Err(Box::new(Error::with_message(
                ErrorKind::Credential,
                "Cannot use both Shared Key and another Azure Authentication method at the same time",
            )));
        }
        #[cfg(test)]
        (Auth::None, Some(AzureAuthentication::MockCredential)) => {
            warn!("Using mock token credential authentication.");
            credential = Some(auth.unwrap().credential().await.unwrap());
        }
        #[cfg(test)]
        (_, Some(AzureAuthentication::MockCredential)) => {
            return Err(Box::new(Error::with_message(
                ErrorKind::Credential,
                "Cannot use both connection string auth and mock credential at the same time",
            )));
        }
    }

    // Use reqwest v0.13 since Azure SDK only implements HttpClient for reqwest::Client v0.13
    let mut reqwest_builder = reqwest_13::ClientBuilder::new();
    let bypass_proxy = {
        let host = url.host_str().unwrap_or("");
        let port = url.port();
        proxy.no_proxy.matches(host)
            || port
                .map(|p| proxy.no_proxy.matches(&format!("{}:{}", host, p)))
                .unwrap_or(false)
    };
    if bypass_proxy || !proxy.enabled {
        // Ensure no proxy (and disable any potential system proxy auto-detection)
        reqwest_builder = reqwest_builder.no_proxy();
    } else {
        if let Some(http) = &proxy.http {
            let p = reqwest_13::Proxy::http(http)
                .map_err(|e| format!("Invalid HTTP proxy URL: {e}"))?;
            // If credentials are embedded in the proxy URL, reqwest will handle them.
            reqwest_builder = reqwest_builder.proxy(p);
        }
        if let Some(https) = &proxy.https {
            let p = reqwest_13::Proxy::https(https)
                .map_err(|e| format!("Invalid HTTPS proxy URL: {e}"))?;
            // If credentials are embedded in the proxy URL, reqwest will handle them.
            reqwest_builder = reqwest_builder.proxy(p);
        }
    }

    if let Some(AzureBlobTlsConfig { ca_file }) = &tls
        && let Some(ca_file) = ca_file
    {
        let mut buf = Vec::new();
        File::open(ca_file)?.read_to_end(&mut buf)?;
        let cert = reqwest_13::Certificate::from_pem(&buf)?;

        warn!("Adding TLS root certificate from {}", ca_file.display());
        reqwest_builder = reqwest_builder.add_root_certificate(cert);
    }

    options.client_options.transport = Some(azure_core::http::Transport::new(std::sync::Arc::new(
        reqwest_builder
            .build()
            .map_err(|e| format!("Failed to build reqwest client: {e}"))?,
    )));
    let client =
        BlobContainerClient::new(url, credential, Some(options)).map_err(|e| format!("{e}"))?;
    Ok(Arc::new(client))
}
