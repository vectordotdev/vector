use lookup::{lookup_v2::OptionalValuePath, OwnedValuePath};
use openssl::{base64, pkey};

use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;
use vector_core::{config::log_schema, schema};
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

fn default_host() -> String {
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
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
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
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
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
    fn build_shared_key(&self) -> crate::Result<pkey::PKey<pkey::Private>> {
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

    async fn build_inner(
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
        let request_settings = self.request.unwrap_with(&Default::default());
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::{future::ready, stream};
    use http::Response;
    use hyper::body;
    use lookup::PathPrefix;
    use openssl::{hash, sign};
    use tokio::time::timeout;

    use super::*;
    use crate::{
        event::LogEvent,
        test_util::{
            components::{run_and_assert_sink_compliance, SINK_TAGS},
            http::{always_200_response, spawn_blackhole_http_server},
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AzureMonitorLogsConfig>();
    }

    #[tokio::test]
    async fn component_spec_compliance() {
        let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

        let config = AzureMonitorLogsConfig {
            shared_key: "ZnNkO2Zhc2RrbGZqYXNkaixmaG5tZXF3dWlsamtmYXNjZmouYXNkbmZrbHFhc2ZtYXNrbA=="
                .to_string()
                .into(),
            ..Default::default()
        };

        let context = SinkContext::default();
        let (sink, _healthcheck) = config
            .build_inner(context, mock_endpoint.into())
            .await
            .unwrap();

        let event = Event::Log(LogEvent::from("simple message"));
        run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;
    }

    #[tokio::test]
    async fn fails_missing_creds() {
        let config: AzureMonitorLogsConfig = toml::from_str(
            r#"
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            shared_key = ""
            log_type = "Vector"
            azure_resource_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
        "#,
        )
        .unwrap();
        if config.build(SinkContext::default()).await.is_ok() {
            panic!("config.build failed to error");
        }
    }

    #[test]
    fn correct_host() {
        let config_default = toml::from_str::<AzureMonitorLogsConfig>(
            r#"
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            shared_key = "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="
            log_type = "Vector"
        "#,
        )
        .expect("Config parsing failed without custom host");
        assert_eq!(config_default.host, default_host());

        let config_cn = toml::from_str::<AzureMonitorLogsConfig>(
            r#"
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            shared_key = "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="
            log_type = "Vector"
            host = "ods.opinsights.azure.cn"
        "#,
        )
        .expect("Config parsing failed with .cn custom host");
        assert_eq!(config_cn.host, "ods.opinsights.azure.cn");
    }

    #[tokio::test]
    async fn fails_invalid_base64() {
        let config: AzureMonitorLogsConfig = toml::from_str(
            r#"
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            shared_key = "1Qs77Vz40+iDMBBTRmROKJwnEX"
            log_type = "Vector"
            azure_resource_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
        "#,
        )
        .unwrap();
        if config.build(SinkContext::default()).await.is_ok() {
            panic!("config.build failed to error");
        }
    }

    #[test]
    fn fails_config_missing_fields() {
        toml::from_str::<AzureMonitorLogsConfig>(
            r#"
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            shared_key = "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="
            azure_resource_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
        "#,
        )
        .expect_err("Config parsing failed to error with missing log_type");

        toml::from_str::<AzureMonitorLogsConfig>(
            r#"
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            log_type = "Vector"
            azure_resource_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
        "#,
        )
        .expect_err("Config parsing failed to error with missing shared_key");

        toml::from_str::<AzureMonitorLogsConfig>(
            r#"
            shared_key = "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="
            log_type = "Vector"
        "#,
        )
        .expect_err("Config parsing failed to error with missing customer_id");
    }

    fn insert_timestamp_kv(log: &mut LogEvent) -> (String, String) {
        let now = chrono::Utc::now();

        let timestamp_key = log_schema().timestamp_key().unwrap();
        let timestamp_value = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        log.insert((PathPrefix::Event, timestamp_key), now);

        (timestamp_key.to_string(), timestamp_value)
    }

    fn build_authorization_header_value(
        shared_key: &pkey::PKey<pkey::Private>,
        customer_id: &str,
        rfc1123date: &str,
        len: usize,
    ) -> crate::Result<String> {
        let string_to_hash =
            format!("POST\n{len}\napplication/json\nx-ms-date:{rfc1123date}\n/api/logs");
        let mut signer = sign::Signer::new(hash::MessageDigest::sha256(), shared_key)?;
        signer.update(string_to_hash.as_bytes())?;

        let signature = signer.sign_to_vec()?;
        let signature_base64 = base64::encode_block(&signature);

        Ok(format!("SharedKey {customer_id}:{signature_base64}"))
    }

    #[tokio::test]
    async fn correct_request() {
        let config: AzureMonitorLogsConfig = toml::from_str(
            r#"
            # random GUID and random 64 Base-64 encoded bytes
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            shared_key = "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="
            log_type = "Vector"
            azure_resource_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
        "#,
        )
        .unwrap();

        let mut log1 = [("message", "hello")].iter().copied().collect::<LogEvent>();
        let (timestamp_key1, timestamp_value1) = insert_timestamp_kv(&mut log1);

        let mut log2 = [("message", "world")].iter().copied().collect::<LogEvent>();
        let (timestamp_key2, timestamp_value2) = insert_timestamp_kv(&mut log2);

        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let mock_endpoint = spawn_blackhole_http_server(move |request| {
            let tx = tx.clone();
            async move {
                tx.send(request).await.unwrap();
                Ok(Response::new(hyper::Body::empty()))
            }
        })
        .await;

        let context = SinkContext::default();
        let (sink, _healthcheck) = config
            .build_inner(context, mock_endpoint.into())
            .await
            .unwrap();

        run_and_assert_sink_compliance(sink, stream::iter(vec![log1, log2]), &SINK_TAGS).await;

        let request = timeout(Duration::from_millis(500), rx.recv())
            .await
            .unwrap()
            .unwrap();

        let (parts, body) = request.into_parts();
        assert_eq!(&parts.method.to_string(), "POST");

        let body = body::to_bytes(body).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body[..]).unwrap();
        let expected_json = serde_json::json!([
            {
                timestamp_key1: timestamp_value1,
                "message": "hello"
            },
            {
                timestamp_key2: timestamp_value2,
                "message": "world"
            }
        ]);
        assert_eq!(json, expected_json);

        let headers = parts.headers;

        let rfc1123date = headers.get("x-ms-date").unwrap();
        let shared_key = config.build_shared_key().unwrap();
        let auth_expected = build_authorization_header_value(
            &shared_key,
            &config.customer_id,
            rfc1123date.to_str().unwrap(),
            body.len(),
        )
        .unwrap();
        let authorization = headers.get("authorization").unwrap();
        assert_eq!(authorization.to_str().unwrap(), &auth_expected);

        let log_type = headers.get("log-type").unwrap();
        assert_eq!(log_type.to_str().unwrap(), "Vector");

        let time_generated_field = headers.get("time-generated-field").unwrap();
        let timestamp_key = log_schema().timestamp_key();
        assert_eq!(
            time_generated_field.to_str().unwrap(),
            timestamp_key.unwrap().to_string().as_str()
        );

        let azure_resource_id = headers.get("x-ms-azureresourceid").unwrap();
        assert_eq!(
            azure_resource_id.to_str().unwrap(),
            "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
        );

        assert_eq!(
            &parts.uri.path_and_query().unwrap().to_string(),
            "/api/logs?api-version=2016-04-01"
        );
    }
}
