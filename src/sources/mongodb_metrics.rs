use crate::{
    config::{self, GlobalOptions, SourceConfig, SourceDescription},
    shutdown::ShutdownSignal,
    Pipeline,
};
use futures::{
    FutureExt, TryFutureExt,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
struct MongoDBMetricsConfig {
    endpoint: String,
    #[serde(default = "default_scrape_interval_secs")]
    scrape_interval_secs: u64,
    #[serde(default = "default_namespace")]
    namespace: String,
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
        Ok(Box::new(async move { Ok(()) }.boxed().compat()))
    }

    fn output_type(&self) -> crate::config::DataType {
        config::DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "mongodb_metrics"
    }
}
