use crate::sinks::util::{Compression, BatchConfig, BatchSettings, Buffer, ServiceBuilderExt, TowerRequestConfig};
use crate::sinks::elasticsearch::{ElasticSearchCommon, ElasticSearchMode, ElasticSearchAuth, ElasticSearchCommonMode};
use crate::sinks::util::encoding::{EncodingConfigWithDefault, EncodingConfigFixed};
use crate::config::{SinkConfig, SinkContext, DataType};
use crate::sinks::{Healthcheck, VectorSink};
use crate::template::Template;
use crate::event::{LogEvent, Value, EventRef};
use crate::sinks::util::http::{RequestConfig, HttpRetryLogic};
use indexmap::map::IndexMap;
use crate::rusoto::RegionOrEndpoint;
use crate::tls::TlsOptions;
use std::collections::{HashMap, BTreeMap};
use crate::transforms::metric_to_log::MetricToLogConfig;
use crate::config::log_schema;
use std::convert::TryFrom;
use crate::sinks::elasticsearch::{BatchActionTemplate, IndexTemplate};
use snafu::ResultExt;
use serde::{Serialize, Deserialize};
use crate::internal_events::TemplateRenderingFailed;
use crate::http::HttpClient;
use crate::sinks::elasticsearch::sink::ElasticSearchSink;
use futures::FutureExt;
use crate::sinks::elasticsearch::request_builder::ElasticsearchRequestBuilder;
use vector_core::stream::BatcherSettings;
use std::time::Duration;
use std::num::NonZeroUsize;
use crate::sinks::elasticsearch::service::{ElasticSearchService, HttpRequestBuilder};
use crate::sinks::elasticsearch::encoder::ElasticSearchEncoder;
use tower::ServiceBuilder;
use crate::sinks::elasticsearch::retry::ElasticSearchRetryLogic;

/// The field name for the timestamp required by data stream mode
const DATA_STREAM_TIMESTAMP_KEY: &str = "@timestamp";

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ElasticSearchConfig {
    // Deprecated name
    #[serde(alias = "host")]
    pub endpoint: String,
    // Deprecated, use normal.index instead
    pub index: Option<String>,
    pub doc_type: Option<String>,
    pub id_key: Option<String>,
    pub pipeline: Option<String>,
    #[serde(default)]
    pub mode: ElasticSearchMode,

    #[serde(default)]
    pub compression: Compression,
    #[serde(
    skip_serializing_if = "crate::serde::skip_serializing_if_default",
    default
    )]
    pub encoding: EncodingConfigFixed<ElasticSearchEncoder>,
    // pub encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: RequestConfig,
    pub auth: Option<ElasticSearchAuth>,

    // Deprecated, moved to request.
    pub headers: Option<IndexMap<String, String>>,
    pub query: Option<HashMap<String, String>>,

    pub aws: Option<RegionOrEndpoint>,
    pub tls: Option<TlsOptions>,
    // Deprecated, use normal.bulk_action instead
    pub bulk_action: Option<String>,
    pub normal: Option<NormalConfig>,
    pub data_stream: Option<DataStreamConfig>,
    pub metrics: Option<MetricToLogConfig>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

impl ElasticSearchConfig {
    pub fn bulk_action(&self) -> crate::Result<Option<Template>> {
        Ok(self
            .normal
            .as_ref()
            .and_then(|n| n.bulk_action.as_deref())
            .or_else(|| self.bulk_action.as_deref())
            .map(|value| Template::try_from(value).context(BatchActionTemplate))
            .transpose()?)
    }

    pub fn index(&self) -> crate::Result<Template> {
        let index = self
            .normal
            .as_ref()
            .and_then(|n| n.index.as_deref())
            .or_else(|| self.index.as_deref())
            .map(String::from)
            .unwrap_or_else(NormalConfig::default_index);
        Ok(Template::try_from(index.as_str()).context(IndexTemplate)?)
    }

    pub fn common_mode(&self) -> crate::Result<ElasticSearchCommonMode> {
        match self.mode {
            ElasticSearchMode::Normal => {
                let index = self.index()?;
                let bulk_action = self.bulk_action()?;
                Ok(ElasticSearchCommonMode::Normal { index, bulk_action })
            }
            ElasticSearchMode::DataStream => Ok(ElasticSearchCommonMode::DataStream(
                self.data_stream.clone().unwrap_or_default(),
            )),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub struct NormalConfig {
    bulk_action: Option<String>,
    index: Option<String>,
}

impl NormalConfig {
    fn default_index() -> String {
        "vector-%Y.%m.%d".into()
    }
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
        let map = log.as_map_mut();
        if let Some(value) = map.remove(timestamp_key) {
            map.insert(DATA_STREAM_TIMESTAMP_KEY.into(), value);
        }
    }

    pub fn dtype<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<String> {
        self.dtype
            .render_string(event)
            .map_err(|error| {
                emit!(&TemplateRenderingFailed {
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
                emit!(&TemplateRenderingFailed {
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
                emit!(&TemplateRenderingFailed {
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


        let existing = log
            .as_map_mut()
            .entry("data_stream".into())
            .or_insert_with(|| Value::Map(BTreeMap::new()))
            .as_map_mut();
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
            (
                self.dtype(log)?,
                self.dataset(log)?,
                self.namespace(log)?,
            )
        } else {
            let data_stream = log.get("data_stream").and_then(|ds| ds.as_map());
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
impl SinkConfig for ElasticSearchConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let common = ElasticSearchCommon::parse_config(self)?;

        let http_client = HttpClient::new(common.tls_settings.clone(), cx.proxy())?;
        let batch_settings = BatchSettings::<Buffer>::default()
            .bytes(10_000_000)
            .timeout(1)
            .parse_config(self.batch)?;

        let batch_settings = BatcherSettings::new(
            batch_settings.timeout,
            NonZeroUsize::new(batch_settings.size.bytes).expect("Batch bytes should not be 0"),
            NonZeroUsize::new(batch_settings.size.events).expect("Batch events should not be 0")
        );

        let request_builder = ElasticsearchRequestBuilder {
            compression: self.compression,
            encoder: self.encoding.clone(),
        };

        let request_limits = self.request.tower.unwrap_with(&TowerRequestConfig::default());

        let http_request_builder = HttpRequestBuilder {
            bulk_uri: common.bulk_uri,
            http_request_config: self.request.clone(),
            http_auth: common.authorization,
            query_params: common.query_params,
            region: common.region,
            compression: Compression::None,
            credentials_provider: common.credentials
        };

        let service = ServiceBuilder::new()
            .settings(request_limits, ElasticSearchRetryLogic)
            .service(ElasticSearchService::new(http_client, http_request_builder));

        let sink = ElasticSearchSink {
            batch_settings,
            request_builder,
            compression: self.compression,
            service,
            acker: cx.acker(),
            metric_to_log: common.metric_to_log,
            mode: common.mode,
            id_key_field: self.id_key.clone(),
            doc_type: common.doc_type
        };

        let common = ElasticSearchCommon::parse_config(self)?;
        let client = HttpClient::new(common.tls_settings.clone(), cx.proxy())?;
        let healthcheck = common.healthcheck(client.clone()).boxed();
        let stream = VectorSink::Stream(Box::new(sink));
        Ok((stream, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "elasticsearch"
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{Event, Metric, MetricKind, MetricValue, Value},
        sinks::util::retries::{RetryAction, RetryLogic},
    };
    use bytes::Bytes;
    use http::{Response, StatusCode};
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ElasticSearchConfig>();
    }

    #[test]
    fn parse_aws_auth() {
        toml::from_str::<ElasticSearchConfig>(
            r#"
            endpoint = ""
            auth.strategy = "aws"
            auth.assume_role = "role"
        "#,
        )
            .unwrap();

        toml::from_str::<ElasticSearchConfig>(
            r#"
            endpoint = ""
            auth.strategy = "aws"
        "#,
        )
            .unwrap();
    }

    #[test]
    fn parse_mode() {
        let config = toml::from_str::<ElasticSearchConfig>(
            r#"
            endpoint = ""
            mode = "data_stream"
            data_stream.type = "synthetics"
        "#,
        )
            .unwrap();
        assert!(matches!(config.mode, ElasticSearchMode::DataStream));
        assert!(config.data_stream.is_some());
    }
}
