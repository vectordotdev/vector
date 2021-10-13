use crate::sinks::util::{Buffer, Compression, BatchConfig};
use crate::sinks::elasticsearch::{ElasticSearchCommon, ElasticSearchMode, ElasticSearchAuth, ElasticSearchCommonMode};
use crate::sinks::util::encoding::EncodingConfigWithDefault;
use crate::config::{SinkConfig, SinkContext, DataType};
use crate::sinks::{Healthcheck, VectorSink};
use crate::template::Template;
use crate::event::{LogEvent, Event, Value, EventRef};
use crate::sinks::util::http::RequestConfig;
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
    pub encoding: EncodingConfigWithDefault<Encoding>,
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
    dtype: Template,
    #[serde(default = "DataStreamConfig::default_dataset")]
    dataset: Template,
    #[serde(default = "DataStreamConfig::default_namespace")]
    namespace: Template,
    #[serde(default = "DataStreamConfig::default_auto_routing")]
    auto_routing: bool,
    #[serde(default = "DataStreamConfig::default_sync_fields")]
    sync_fields: bool,
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


        todo!()
        // let common = ElasticSearchCommon::parse_config(self)?;
        // let client = HttpClient::new(common.tls_settings.clone(), cx.proxy())?;
        //
        // let healthcheck = common.healthcheck(client.clone()).boxed();
        //
        // let common = ElasticSearchCommon::parse_config(self)?;
        // let compression = common.compression;
        // let batch = BatchSettings::default()
        //     .bytes(bytesize::mib(10u64))
        //     .timeout(1)
        //     .parse_config(self.batch)?;
        // let request = self
        //     .request
        //     .tower
        //     .unwrap_with(&TowerRequestConfig::default());
        //
        // let sink = BatchedHttpSink::with_logic(
        //     common,
        //     Buffer::new(batch.size, compression),
        //     ElasticSearchRetryLogic,
        //     request,
        //     batch.timeout,
        //     client,
        //     cx.acker(),
        //     ElasticSearchServiceLogic,
        // )
        //     .sink_map_err(|error| error!(message = "Fatal elasticsearch sink error.", %error));
        //
        // Ok((super::VectorSink::Sink(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "elasticsearch"
    }
}
