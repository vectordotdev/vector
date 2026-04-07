use std::sync::Arc;

use azure_storage_blob::BlobContainerClient;
use tower::ServiceBuilder;
use vector_lib::{
    codecs::{JsonSerializerConfig, NewlineDelimitedEncoderConfig, encoding::Framer},
    configurable::configurable_component,
    sensitive_string::SensitiveString,
};

use super::request_builder::AzureBlobRequestOptions;
use crate::{
    Result,
    codecs::{Encoder, EncodingConfigWithFraming, SinkType},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{
        Healthcheck, VectorSink,
        azure_common::{
            self, config::AzureAuthentication, config::AzureBlobRetryLogic,
            config::AzureBlobTlsConfig, service::AzureBlobService, sink::AzureBlobSink,
        },
        util::{
            BatchConfig, BulkSizeBasedDefaultBatchSettings, Compression, ServiceBuilderExt,
            TowerRequestConfig, partitioner::KeyPartitioner, service::TowerRequestConfigDefaults,
        },
    },
    template::Template,
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

        let client = azure_common::config::build_client(
            self.auth.clone(),
            connection_string.clone(),
            self.container_name.clone(),
            cx.proxy(),
            self.tls.clone(),
        )
        .await?;

        let healthcheck = azure_common::config::build_healthcheck(
            self.container_name.clone(),
            Arc::clone(&client),
        )?;
        let sink = self.build_processor(client)?;
        Ok((sink, healthcheck))
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
        Ok(KeyPartitioner::new(self.blob_prefix.clone(), None))
    }
}
