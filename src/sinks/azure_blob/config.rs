use std::{convert::TryInto, sync::Arc};

use azure_storage_blobs::prelude::*;
use codecs::{encoding::Framer, JsonSerializerConfig, NewlineDelimitedEncoderConfig};
use tower::ServiceBuilder;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;

use super::request_builder::AzureBlobRequestOptions;
use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{
        azure_common::{
            self, config::AzureBlobRetryLogic, service::AzureBlobService, sink::AzureBlobSink,
        },
        util::{
            partitioner::KeyPartitioner, BatchConfig, BulkSizeBasedDefaultBatchSettings,
            Compression, ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    Result,
};

/// Configuration for the `azure_blob` sink.
#[configurable_component(sink("azure_blob"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AzureBlobSinkConfig {
    /// The Azure Blob Storage Account connection string.
    ///
    /// Authentication with access key is the only supported authentication method.
    ///
    /// Either `storage_account`, or this field, must be specified.
    pub connection_string: Option<SensitiveString>,

    /// The Azure Blob Storage Account name.
    ///
    /// Attempts to load credentials for the account in the following ways, in order:
    ///
    /// - read from environment variables ([more information][env_cred_docs])
    /// - looks for a [Managed Identity][managed_ident_docs]
    /// - uses the `az` CLI tool to get an access token ([more information][az_cli_docs])
    ///
    /// Either `connection_string`, or this field, must be specified.
    ///
    /// [env_cred_docs]: https://docs.rs/azure_identity/latest/azure_identity/struct.EnvironmentCredential.html
    /// [managed_ident_docs]: https://docs.microsoft.com/en-us/azure/active-directory/managed-identities-azure-resources/overview
    /// [az_cli_docs]: https://docs.microsoft.com/en-us/cli/azure/account?view=azure-cli-latest#az-account-get-access-token
    pub storage_account: Option<String>,

    /// The Azure Blob Storage Endpoint URL.
    ///
    /// This is used to override the default blob storage endpoint URL in cases where you are using
    /// credentials read from the environment/managed identities or access tokens without using an
    /// explicit connection_string (which already explicitly supports overriding the blob endpoint
    /// URL).
    ///
    /// This may only be used with `storage_account` and will be ignored when used with
    /// `connection_string`.
    pub endpoint: Option<String>,

    /// The Azure Blob Storage Account container name.
    pub(super) container_name: String,

    /// A prefix to apply to all blob keys.
    ///
    /// Prefixes are useful for partitioning objects, such as by creating an blob key that
    /// stores blobs under a particular "directory". If using a prefix for this purpose, it must end
    /// in `/` to act as a directory path. A trailing `/` is **not** automatically added.
    pub blob_prefix: Option<String>,

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
    /// When set to an empty string, no timestamp will be appended to the blob prefix.
    ///
    /// [chrono_strftime_specifiers]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
    pub blob_time_format: Option<String>,

    /// Whether or not to append a UUID v4 token to the end of the blob key.
    ///
    /// The UUID is appended to the timestamp portion of the object key, such that if the blob key
    /// being generated was `date=2022-07-18/1658176486`, setting this field to `true` would result
    /// in an blob key that looked like
    /// `date=2022-07-18/1658176486-30f6652c-71da-4f9f-800d-a1189c47c547`.
    ///
    /// This ensures there are no name collisions, and can be useful in high-volume workloads where
    /// blob keys must be unique.
    pub blob_append_uuid: Option<bool>,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,

    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<BulkSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub(super) acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for AzureBlobSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            connection_string: Some(String::from("DefaultEndpointsProtocol=https;AccountName=some-account-name;AccountKey=some-account-key;").into()),
            storage_account: Some(String::from("some-account-name")),
            container_name: String::from("logs"),
            endpoint: None,
            blob_prefix: Some(String::from("blob")),
            blob_time_format: Some(String::from("%s")),
            blob_append_uuid: Some(true),
            encoding: (Some(NewlineDelimitedEncoderConfig::new()), JsonSerializerConfig::default()).into(),
            compression: Compression::gzip_default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for AzureBlobSinkConfig {
    async fn build(&self, _cx: SinkContext) -> Result<(VectorSink, Healthcheck)> {
        let client = azure_common::config::build_client(
            self.connection_string
                .as_ref()
                .map(|v| v.inner().to_string()),
            self.storage_account.as_ref().map(|v| v.to_string()),
            self.container_name.clone(),
            self.endpoint.clone(),
        )?;

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

const DEFAULT_REQUEST_LIMITS: TowerRequestConfig =
    TowerRequestConfig::const_default().rate_limit_num(250);

const DEFAULT_KEY_PREFIX: &str = "blob/%F/";
const DEFAULT_FILENAME_TIME_FORMAT: &str = "%s";
const DEFAULT_FILENAME_APPEND_UUID: bool = true;

impl AzureBlobSinkConfig {
    pub fn build_processor(&self, client: Arc<ContainerClient>) -> crate::Result<VectorSink> {
        let request_limits = self.request.unwrap_with(&DEFAULT_REQUEST_LIMITS);
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
        let blob_prefix = self
            .blob_prefix
            .as_ref()
            .cloned()
            .unwrap_or_else(|| DEFAULT_KEY_PREFIX.into())
            .try_into()?;
        Ok(KeyPartitioner::new(blob_prefix))
    }
}
