use crate::config::{Config, SourceConfig};
use async_graphql::Object;
use lazy_static::lazy_static;
use std::sync::{Arc, RwLock};

lazy_static! {
    static ref CONFIG: Arc<RwLock<Config>> = Arc::new(RwLock::new(Config::default()));
}

pub struct Source<'a>(&'a Box<dyn SourceConfig>);

#[Object]
impl Source<'_> {
    async fn name(&self) -> &'static str {
        self.0.source_type()
    }
}

#[derive(Default)]
pub struct TopologyQuery;

#[Object]
impl TopologyQuery {
    async fn sources(&self) -> Vec<String> {
        CONFIG
            .read()
            .unwrap()
            .sources
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }
}

/// Update the 'global' configuration that will be consumed by topology queries
pub fn update_config(config: Config) {
    *CONFIG.write().unwrap() = config;
}
