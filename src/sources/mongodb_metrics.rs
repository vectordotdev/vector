use crate::{
    config::{self, GlobalOptions, SourceConfig, SourceDescription},
    event::metric::{Metric, MetricKind, MetricValue},
    shutdown::ShutdownSignal,
    Event, Pipeline,
};
use chrono::Utc;
use futures::{
    compat::{Future01CompatExt, Sink01CompatExt},
    FutureExt, Stream, StreamExt, TryFutureExt,
};
use futures01::Sink;
use mongodb::{options::ClientOptions, Client};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
use tokio::time::{interval, Duration};
use url::Url;

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
    client: Client,
    namespace: String,
    tags: BTreeMap<String, String>,
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
        let client_options = ClientOptions::parse(endpoint).await?;

        let mut tags: BTreeMap<String, String> = BTreeMap::new();
        // TODO: Works only in Standalone mode
        tags.insert("host".into(), client_options.hosts[0].hostname.clone());
        tags.insert("endpoint".into(), sanitize_endpoint(endpoint)?);

        Ok(Arc::new(Self {
            client: Client::with_options(client_options)?,
            namespace: namespace.to_owned(),
            tags,
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
            timestamp: Some(Utc::now()),
            tags: Some(self.tags.clone()),
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge { value: 1.0 },
        }))
    }
}

// TODO: need to be improved (unwrap, standalone mode)
fn sanitize_endpoint(endpoint: &str) -> crate::Result<String> {
    let mut url = Url::parse(endpoint)?;
    url.set_username("").unwrap();
    url.set_password(None).unwrap();
    Ok(url.to_string())
}
