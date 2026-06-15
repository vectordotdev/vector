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
            config::AzureBlobTlsConfig, config::AzureBlobType, service::AzureBlobService,
            sink::AzureBlobSink,
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
    /// Blob keys are appended with a timestamp that reflects when the blob is sent to
    /// Azure Blob Storage. The resulting blob key is functionally equivalent to joining
    /// the blob prefix with the formatted timestamp, such as `date=2022-07-18/1658176486`.
    ///
    /// This would represent a `blob_prefix` set to `date=%F/` and the timestamp of Mon Jul 18 2022
    /// 20:34:44 GMT+0000, with the `blob_time_format` set to `%s`, which renders timestamps in
    /// seconds since the Unix epoch.
    ///
    /// Supports the common [`strftime`][chrono_strftime_specifiers] specifiers found in most
    /// languages.
    ///
    /// When set to an empty string, no timestamp is appended to the blob prefix.
    ///
    /// The default value depends on `blob_type`:
    /// - `block`: `%s` (Unix epoch seconds) — each batch gets a unique timestamp.
    /// - `append`: `%Y-%m-%d` (ISO date) — batches within the same day share the same blob.
    ///
    /// [chrono_strftime_specifiers]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
    #[configurable(metadata(docs::syntax_override = "strftime"))]
    pub blob_time_format: Option<String>,

    /// Whether or not to append a UUID v4 token to the end of the blob key.
    ///
    /// The UUID is appended to the timestamp portion of the object key, such that if the blob key
    /// generated is `date=2022-07-18/1658176486`, setting this field to `true` results
    /// in a blob key that looks like
    /// `date=2022-07-18/1658176486-30f6652c-71da-4f9f-800d-a1189c47c547`.
    ///
    /// The default value depends on `blob_type`:
    /// - `block`: `true` — guarantees unique blob names across concurrent writers.
    /// - `append`: `false` — multiple batches must share the same blob name to append to it.
    ///   Set to `true` only if you intentionally want each flush to target a distinct append blob.
    pub blob_append_uuid: Option<bool>,

    /// The type of blob to use when writing to Azure Blob Storage.
    ///
    /// - `block` (default): a new uniquely-named blob per batch.
    ///   `blob_append_uuid` defaults to `true`; `blob_time_format` defaults to `%s`.
    /// - `append`: each batch appends to the same blob.
    ///   `blob_append_uuid` defaults to `false`; `blob_time_format` defaults to `%Y-%m-%d`.
    ///   Multiple batches within the same time window write to the same blob.
    ///
    /// **Batch size limit for `append` mode**: Azure limits each `append_block` call to 4 MiB
    /// (4,194,304 bytes). `batch.max_bytes` automatically defaults to `4194304` when
    /// `blob_type` is `append` and the setting is not explicitly configured.
    /// Setting `batch.max_bytes` above `4194304` with `blob_type: append` is an error and
    /// Vector will fail to start.
    ///
    /// When `blob_type` is `append` and compression is enabled, each batch is compressed as an
    /// independent frame and appended to the blob. The result is a series of concatenated
    /// compressed frames. Use decompressors that support multi-stream decompression
    /// (e.g., `gunzip`, `zstd -d`).
    #[configurable(derived)]
    #[serde(default)]
    pub blob_type: AzureBlobType,

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
            blob_type: AzureBlobType::Block,
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
const DEFAULT_APPEND_BLOB_TIME_FORMAT: &str = "%Y-%m-%d";
const DEFAULT_APPEND_BLOB_APPEND_UUID: bool = false;
const APPEND_BLOB_MAX_BLOCK_BYTES: usize = 4 * 1024 * 1024;

impl AzureBlobSinkConfig {
    pub fn build_processor(&self, client: Arc<BlobContainerClient>) -> crate::Result<VectorSink> {
        let request_limits = self.request.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_limits, AzureBlobRetryLogic)
            .service(AzureBlobService::new(client));

        // Configure our partitioning/batching.
        // For append blobs, if the user hasn't set max_bytes, default it to the 4 MiB
        // Azure hard limit instead of inheriting the 10 MB BulkSizeBasedDefault.
        // This prevents a startup failure when blob_type = append is used out of the box.
        let mut batch = self.batch;
        if self.blob_type == AzureBlobType::Append && batch.max_bytes.is_none() {
            batch.max_bytes = Some(APPEND_BLOB_MAX_BLOCK_BYTES);
        }
        let validated_batch = batch.validate()?;
        let validated_batch = if self.blob_type == AzureBlobType::Append {
            validated_batch.limit_max_bytes(APPEND_BLOB_MAX_BLOCK_BYTES)?
        } else {
            validated_batch
        };
        let batcher_settings = validated_batch.into_batcher_settings()?;

        let (default_append_uuid, default_time_format) = match self.blob_type {
            AzureBlobType::Block => (DEFAULT_FILENAME_APPEND_UUID, DEFAULT_FILENAME_TIME_FORMAT),
            AzureBlobType::Append => (
                DEFAULT_APPEND_BLOB_APPEND_UUID,
                DEFAULT_APPEND_BLOB_TIME_FORMAT,
            ),
        };
        let blob_time_format = self
            .blob_time_format
            .as_deref()
            .unwrap_or(default_time_format)
            .to_string();
        let blob_append_uuid = self.blob_append_uuid.unwrap_or(default_append_uuid);

        let transformer = self.encoding.transformer();
        let (framer, serializer) = self.encoding.build(SinkType::MessageBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let request_options = AzureBlobRequestOptions {
            container_name: self.container_name.clone(),
            blob_time_format,
            blob_append_uuid,
            blob_type: self.blob_type,
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
