use crate::{
    config::{self, GlobalOptions, SourceConfig, SourceDescription},
    event::metric::{Metric, MetricKind, MetricValue},
    shutdown::ShutdownSignal,
    Event, Pipeline,
};
use futures::{
    compat::{Future01CompatExt, Sink01CompatExt},
    FutureExt, Stream, StreamExt, TryFutureExt,
};
use futures01::Sink;
use mongodb::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{interval, Duration};

#[derive(Deserialize, Serialize, Clone, Debug)]
struct MongoDBMetricsConfig {
    endpoint: String,
    #[serde(default = "default_scrape_interval_secs")]
    scrape_interval_secs: u64,
    #[serde(default = "default_namespace")]
    namespace: String,
}

#[derive(Debug)]
struct MongoDBMetrics {
    pub client: Client,
    pub namespace: String,
}

pub fn default_scrape_interval_secs() -> u64 {
    15
}

pub fn default_namespace() -> String {
    "mongodb".to_string()
}

inventory::submit! {
    SourceDescription::new_without_default::<MongoDBMetricsConfig>("mongodb_metrics")
}

#[async_trait::async_trait]
#[typetag::serde(name = "mongodb_metrics")]
impl SourceConfig for MongoDBMetricsConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        let mongodb = MongoDBMetrics::new(&self.endpoint, &self.namespace).await?;

        let out = out
            .sink_map_err(|e| error!("error sending metric: {:?}", e))
            .sink_compat();

        let task = interval(Duration::from_secs(self.scrape_interval_secs))
            .take_until(shutdown.compat())
            .filter_map(move |_| {
                let mongodb = Arc::clone(&mongodb);
                async { Some(mongodb.collect().await) }
            })
            .flatten()
            .map(Event::Metric)
            .map(Ok)
            .forward(out)
            .inspect(|_| info!("finished sending"));

        Ok(Box::new(task.boxed().compat()))
    }

    fn output_type(&self) -> crate::config::DataType {
        config::DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "mongodb_metrics"
    }
}

impl MongoDBMetrics {
    async fn new(endpoint: &str, namespace: &str) -> crate::Result<Arc<MongoDBMetrics>> {
        Ok(Arc::new(Self {
            client: Client::with_uri_str(endpoint).await?,
            namespace: namespace.to_owned(),
        }))
    }

    fn encode_namespace(&self, name: &str) -> String {
        match self.namespace.as_str() {
            "" => name.to_string(),
            _ => format!("{}_{}", self.namespace, name),
        }
    }

    async fn collect(self: Arc<Self>) -> impl Stream<Item = Metric> {
        futures::stream::once(futures::future::ready(Metric {
            name: self.encode_namespace("up"),
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge { value: 1.0 },
        }))
    }
}
