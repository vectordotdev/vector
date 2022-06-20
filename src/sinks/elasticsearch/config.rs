use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    time::Duration,
};

use futures::{stream, FutureExt};
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use tower::{discover::Change, ServiceBuilder};

use crate::{
    aws::RegionOrEndpoint,
    config::{log_schema, AcknowledgementsConfig, DataType, Input, SinkConfig, SinkContext},
    event::{EventRef, LogEvent, Value},
    http::HttpClient,
    internal_events::TemplateRenderingError,
    sinks::util::encoding::Transformer,
    sinks::{
        elasticsearch::{
            retry::ElasticsearchRetryLogic,
            service::{ElasticsearchService, HttpRequestBuilder},
            sink::ElasticsearchSink,
            BatchActionTemplateSnafu, ElasticsearchAuth, ElasticsearchCommon,
            ElasticsearchCommonMode, ElasticsearchMode, IndexTemplateSnafu,
        },
        util::{
            http::RequestConfig, BatchConfig, Compression, RealtimeSizeBasedDefaultBatchSettings,
            ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    template::Template,
    tls::TlsConfig,
    transforms::metric_to_log::MetricToLogConfig,
};
use lookup::path;

/// The field name for the timestamp required by data stream mode
pub const DATA_STREAM_TIMESTAMP_KEY: &str = "@timestamp";
pub const REACTIVATE_DELAY_SECONDS_DEFAULT: u64 = 5; // 5 seconds

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ElasticsearchConfig {
    pub endpoint: String,

    pub doc_type: Option<String>,
    #[serde(default)]
    pub suppress_type_name: bool,
    pub id_key: Option<String>,
    pub pipeline: Option<String>,
    #[serde(default)]
    pub mode: ElasticsearchMode,

    #[serde(default)]
    pub compression: Compression,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: Transformer,

    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,
    #[serde(default)]
    pub request: RequestConfig,
    pub auth: Option<ElasticsearchAuth>,
    pub query: Option<HashMap<String, String>>,
    pub aws: Option<RegionOrEndpoint>,
    pub tls: Option<TlsConfig>,
    pub distribution: Option<DistributionConfig>,

    #[serde(alias = "normal")]
    pub bulk: Option<BulkConfig>,
    pub data_stream: Option<DataStreamConfig>,
    pub metrics: Option<MetricToLogConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl ElasticsearchConfig {
    pub fn bulk_action(&self) -> crate::Result<Option<Template>> {
        Ok(self
            .bulk
            .as_ref()
            .and_then(|n| n.action.as_deref())
            .map(|value| Template::try_from(value).context(BatchActionTemplateSnafu))
            .transpose()?)
    }

    pub fn index(&self) -> crate::Result<Template> {
        let index = self
            .bulk
            .as_ref()
            .and_then(|n| n.index.as_deref())
            .map(String::from)
            .unwrap_or_else(BulkConfig::default_index);
        Ok(Template::try_from(index.as_str()).context(IndexTemplateSnafu)?)
    }

    pub fn common_mode(&self) -> crate::Result<ElasticsearchCommonMode> {
        match self.mode {
            ElasticsearchMode::Bulk => {
                let index = self.index()?;
                let bulk_action = self.bulk_action()?;
                Ok(ElasticsearchCommonMode::Bulk {
                    index,
                    action: bulk_action,
                })
            }
            ElasticsearchMode::DataStream => Ok(ElasticsearchCommonMode::DataStream(
                self.data_stream.clone().unwrap_or_default(),
            )),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub struct BulkConfig {
    pub action: Option<String>,
    pub index: Option<String>,
}

impl BulkConfig {
    fn default_index() -> String {
        "vector-%Y.%m.%d".into()
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct DistributionConfig {
    pub endpoints: Vec<String>,
    pub reactivate_delay_secs: Option<u64>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DataStreamConfig {
    #[serde(rename = "type", default = "DataStreamConfig::default_type")]
    pub dtype: Template,
    #[serde(default = "DataStreamConfig::default_dataset")]
    pub dataset: Template,
    #[serde(default = "DataStreamConfig::default_namespace")]
    pub namespace: Template,
    #[serde(default = "DataStreamConfig::default_auto_routing")]
    pub auto_routing: bool,
    #[serde(default = "DataStreamConfig::default_sync_fields")]
    pub sync_fields: bool,
}

impl Default for DataStreamConfig {
    fn default() -> Self {
        Self {
            dtype: Self::default_type(),
            dataset: Self::default_dataset(),
            namespace: Self::default_namespace(),
            auto_routing: Self::default_auto_routing(),
            sync_fields: Self::default_sync_fields(),
        }
    }
}

impl DataStreamConfig {
    fn default_type() -> Template {
        Template::try_from("logs").expect("couldn't build default type template")
    }

    fn default_dataset() -> Template {
        Template::try_from("generic").expect("couldn't build default dataset template")
    }

    fn default_namespace() -> Template {
        Template::try_from("default").expect("couldn't build default namespace template")
    }

    const fn default_auto_routing() -> bool {
        true
    }

    const fn default_sync_fields() -> bool {
        true
    }

    pub fn remap_timestamp(&self, log: &mut LogEvent) {
        // we keep it if the timestamp field is @timestamp
        let timestamp_key = log_schema().timestamp_key();
        if timestamp_key == DATA_STREAM_TIMESTAMP_KEY {
            return;
        }

        if let Some(value) = log.remove(timestamp_key) {
            log.insert(path!(DATA_STREAM_TIMESTAMP_KEY), value);
        }
    }

    pub fn dtype<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<String> {
        self.dtype
            .render_string(event)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("data_stream.type"),
                    drop_event: true,
                });
            })
            .ok()
    }

    pub fn dataset<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<String> {
        self.dataset
            .render_string(event)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("data_stream.dataset"),
                    drop_event: true,
                });
            })
            .ok()
    }

    pub fn namespace<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<String> {
        self.namespace
            .render_string(event)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("data_stream.namespace"),
                    drop_event: true,
                });
            })
            .ok()
    }

    pub fn sync_fields(&self, log: &mut LogEvent) {
        if !self.sync_fields {
            return;
        }

        let dtype = self.dtype(&*log);
        let dataset = self.dataset(&*log);
        let namespace = self.namespace(&*log);

        if log.as_map().is_none() {
            *log.value_mut() = Value::Object(BTreeMap::new());
        }
        let existing = log
            .as_map_mut()
            .expect("must be a map")
            .entry("data_stream".into())
            .or_insert_with(|| Value::Object(BTreeMap::new()))
            .as_object_mut_unwrap();

        if let Some(dtype) = dtype {
            existing
                .entry("type".into())
                .or_insert_with(|| dtype.into());
        }
        if let Some(dataset) = dataset {
            existing
                .entry("dataset".into())
                .or_insert_with(|| dataset.into());
        }
        if let Some(namespace) = namespace {
            existing
                .entry("namespace".into())
                .or_insert_with(|| namespace.into());
        }
    }

    pub fn index(&self, log: &LogEvent) -> Option<String> {
        let (dtype, dataset, namespace) = if !self.auto_routing {
            (self.dtype(log)?, self.dataset(log)?, self.namespace(log)?)
        } else {
            let data_stream = log.get("data_stream").and_then(|ds| ds.as_object());
            let dtype = data_stream
                .and_then(|ds| ds.get("type"))
                .map(|value| value.to_string_lossy())
                .or_else(|| self.dtype(log))?;
            let dataset = data_stream
                .and_then(|ds| ds.get("dataset"))
                .map(|value| value.to_string_lossy())
                .or_else(|| self.dataset(log))?;
            let namespace = data_stream
                .and_then(|ds| ds.get("namespace"))
                .map(|value| value.to_string_lossy())
                .or_else(|| self.namespace(log))?;
            (dtype, dataset, namespace)
        };
        Some(format!("{}-{}-{}", dtype, dataset, namespace))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "elasticsearch")]
impl SinkConfig for ElasticsearchConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let common = ElasticsearchCommon::parse_config(self).await?;
        let client = HttpClient::new(common.tls_settings.clone(), cx.proxy())?;

        let request_limits = self
            .request
            .tower
            .unwrap_with(&TowerRequestConfig::default());

        let stream = if let Some(distribution) = self.distribution.as_ref() {
            // Distributed services

            let reactivate_delay = Duration::from_secs(
                distribution
                    .reactivate_delay_secs
                    .unwrap_or(REACTIVATE_DELAY_SECONDS_DEFAULT),
            );

            // Multiply configuration, one for each endpoint
            let commons = Some(common.clone())
                .into_iter()
                .chain(ElasticsearchCommon::parse_endpoints(self).await?);

            let services = commons
                .into_iter()
                .map(|common| {
                    let http_request_builder = HttpRequestBuilder::new(&common, self);
                    let service = ElasticsearchService::new(client.clone(), http_request_builder);

                    let client = client.clone();
                    let healthcheck = move || {
                        common
                            .clone()
                            .healthcheck(client.clone())
                            .map(|result| result.is_ok())
                            .boxed()
                    };

                    (service, healthcheck)
                })
                .enumerate()
                .map(|(i, service)| Ok(Change::Insert(i, service)))
                .collect::<Vec<_>>();

            let service = request_limits.distributed_service(
                ElasticsearchRetryLogic,
                stream::iter(services),
                reactivate_delay,
            );

            let sink = ElasticsearchSink::new(&common, self, cx.acker(), service)?;
            VectorSink::from_event_streamsink(sink)
        } else {
            // Single service

            let service =
                ElasticsearchService::new(client.clone(), HttpRequestBuilder::new(&common, self));

            let service = ServiceBuilder::new()
                .settings(request_limits, ElasticsearchRetryLogic)
                .service(service);

            let sink = ElasticsearchSink::new(&common, self, cx.acker(), service)?;
            VectorSink::from_event_streamsink(sink)
        };

        let healthcheck = common.healthcheck(client).boxed();
        Ok((stream, healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Metric | DataType::Log)
    }

    fn sink_type(&self) -> &'static str {
        "elasticsearch"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ElasticsearchConfig>();
    }

    #[test]
    fn parse_aws_auth() {
        toml::from_str::<ElasticsearchConfig>(
            r#"
            endpoint = ""
            auth.strategy = "aws"
            auth.assume_role = "role"
        "#,
        )
        .unwrap();

        toml::from_str::<ElasticsearchConfig>(
            r#"
            endpoint = ""
            auth.strategy = "aws"
        "#,
        )
        .unwrap();
    }

    #[test]
    fn parse_mode() {
        let config = toml::from_str::<ElasticsearchConfig>(
            r#"
            endpoint = ""
            mode = "data_stream"
            data_stream.type = "synthetics"
        "#,
        )
        .unwrap();
        assert!(matches!(config.mode, ElasticsearchMode::DataStream));
        assert!(config.data_stream.is_some());
    }

    #[test]
    fn parse_distribution() {
        toml::from_str::<ElasticsearchConfig>(
            r#"
            endpoint = ""
            distribution.endpoints = []
            distribution.reactivate_delay_secs = 10
        "#,
        )
        .unwrap();
    }
}
