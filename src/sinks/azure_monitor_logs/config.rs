use openssl::{base64, pkey};
use vector_lib::lookup::{lookup_v2::OptionalValuePath, OwnedValuePath};

use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;
use vector_lib::{config::log_schema, schema};
use vrl::value::Kind;

use crate::{
    http::{get_http_scheme_from_uri, HttpClient},
    sinks::{
        prelude::*,
        util::{http::HttpStatusRetryLogic, RealtimeSizeBasedDefaultBatchSettings, UriSerde},
    },
};

use super::{
    service::{AzureMonitorLogsResponse, AzureMonitorLogsService},
    sink::AzureMonitorLogsSink,
};

/// Max number of bytes in request body
const MAX_BATCH_SIZE: usize = 30 * 1024 * 1024;

pub(super) fn default_host() -> String {
    "ods.opinsights.azure.com".into()
}

/// Configuration for the `azure_monitor_logs` sink.
#[configurable_component(sink(
    "azure_monitor_logs",
    "Publish log events to the Azure Monitor Logs service."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AzureMonitorLogsConfig {
    /// The [unique identifier][uniq_id] for the Log Analytics workspace.
    ///
    /// [uniq_id]: https://docs.microsoft.com/en-us/azure/azure-monitor/platform/data-collector-api#request-uri-parameters
    #[configurable(metadata(docs::examples = "5ce893d9-2c32-4b6c-91a9-b0887c2de2d6"))]
    #[configurable(metadata(docs::examples = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"))]
    pub customer_id: String,

    /// The [primary or the secondary key][shared_key] for the Log Analytics workspace.
    ///
    /// [shared_key]: https://docs.microsoft.com/en-us/azure/azure-monitor/platform/data-collector-api#authorization
    #[configurable(metadata(
        docs::examples = "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="
    ))]
    #[configurable(metadata(docs::examples = "${AZURE_MONITOR_SHARED_KEY_ENV_VAR}"))]
    pub shared_key: SensitiveString,

    /// The [record type][record_type] of the data that is being submitted.
    ///
    /// Can only contain letters, numbers, and underscores (_), and may not exceed 100 characters.
    ///
    /// [record_type]: https://docs.microsoft.com/en-us/azure/azure-monitor/platform/data-collector-api#request-headers
    #[configurable(validation(pattern = "[a-zA-Z0-9_]{1,100}"))]
    #[configurable(metadata(docs::examples = "MyTableName"))]
    #[configurable(metadata(docs::examples = "MyRecordType"))]
    pub log_type: String,

    /// The [Resource ID][resource_id] of the Azure resource the data should be associated with.
    ///
    /// [resource_id]: https://docs.microsoft.com/en-us/azure/azure-monitor/platform/data-collector-api#request-headers
    #[configurable(metadata(
        docs::examples = "/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/otherResourceGroup/providers/Microsoft.Storage/storageAccounts/examplestorage"
    ))]
    #[configurable(metadata(
        docs::examples = "/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/examplegroup/providers/Microsoft.SQL/servers/serverName/databases/databaseName"
    ))]
    pub azure_resource_id: Option<String>,

    /// [Alternative host][alt_host] for dedicated Azure regions.
    ///
    /// [alt_host]: https://docs.azure.cn/en-us/articles/guidance/developerdifferences#check-endpoints-in-azure
    #[configurable(metadata(docs::examples = "ods.opinsights.azure.us"))]
    #[configurable(metadata(docs::examples = "ods.opinsights.azure.cn"))]
    #[serde(default = "default_host")]
    pub(super) host: String,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub encoding: Transformer,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    /// Use this option to customize the log field used as [`TimeGenerated`][1] in Azure.
    ///
    /// The setting of `log_schema.timestamp_key`, usually `timestamp`, is used here by default.
    /// This field should be used in rare cases where `TimeGenerated` should point to a specific log
    /// field. For example, use this field to set the log field `source_timestamp` as holding the
    /// value that should be used as `TimeGenerated` on the Azure side.
    ///
    /// [1]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/log-standard-columns#timegenerated
    #[configurable(metadata(docs::examples = "time_generated"))]
    pub time_generated_key: Option<OptionalValuePath>,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl Default for AzureMonitorLogsConfig {
    fn default() -> Self {
        Self {
            customer_id: "my-customer-id".to_string(),
            shared_key: Default::default(),
            log_type: "MyRecordType".to_string(),
            azure_resource_id: None,
            host: default_host(),
            encoding: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            time_generated_key: None,
            tls: None,
            acknowledgements: Default::default(),
        }
    }
}

impl AzureMonitorLogsConfig {
    pub(super) fn build_shared_key(&self) -> crate::Result<pkey::PKey<pkey::Private>> {
        if self.shared_key.inner().is_empty() {
            return Err("shared_key cannot be an empty string".into());
        }
        let shared_key_bytes = base64::decode_block(self.shared_key.inner())?;
        let shared_key = pkey::PKey::hmac(&shared_key_bytes)?;
        Ok(shared_key)
    }

    fn get_time_generated_key(&self) -> Option<OwnedValuePath> {
        self.time_generated_key
            .clone()
            .and_then(|k| k.path)
            .or_else(|| log_schema().timestamp_key().cloned())
    }

    pub(super) async fn build_inner(
        &self,
        cx: SinkContext,
        endpoint: UriSerde,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoint = endpoint.with_default_parts().uri;
        let protocol = get_http_scheme_from_uri(&endpoint).to_string();

        let batch_settings = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_BATCH_SIZE)?
            .into_batcher_settings()?;

        let shared_key = self.build_shared_key()?;
        let time_generated_key = self.get_time_generated_key();

        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(Some(tls_settings), &cx.proxy)?;

        let service = AzureMonitorLogsService::new(
            client,
            endpoint,
            self.customer_id.clone(),
            self.azure_resource_id.as_deref(),
            &self.log_type,
            time_generated_key.clone(),
            shared_key,
        )?;
        let healthcheck = service.healthcheck();

        let retry_logic =
            HttpStatusRetryLogic::new(|res: &AzureMonitorLogsResponse| res.http_status);
        let request_settings = self.request.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_settings, retry_logic)
            .service(service);

        let sink = AzureMonitorLogsSink::new(
            batch_settings,
            self.encoding.clone(),
            service,
            time_generated_key,
            protocol,
        );

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

impl_generate_config_from_default!(AzureMonitorLogsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "azure_monitor_logs")]
impl SinkConfig for AzureMonitorLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoint = format!("https://{}.{}", self.customer_id, self.host).parse()?;
        self.build_inner(cx, endpoint).await
    }

    fn input(&self) -> Input {
        let requirements =
            schema::Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirements)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}
