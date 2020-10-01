use crate::config::{Config, DataType, SourceConfig};
use async_graphql::Object;
use lazy_static::lazy_static;
use std::sync::{Arc, RwLock};

// #[EnumType]
// pub enum SourceOutputType {
//     Any,
//     Log,
//     Metric,
// }

#[derive(Clone)]
pub struct SourceData {
    name: String,
    output_type: DataType,
}

#[derive(Clone)]
pub struct Source(SourceData);

lazy_static! {
    static ref SOURCES: Arc<RwLock<Vec<Source>>> = Arc::new(RwLock::new(vec![]));
}

#[Object]
impl Source {
    /// Source name
    async fn name(&self) -> String {
        self.0.name.clone()
    }

    // /// The output type given by the source
    // async fn output_type(&self) -> SourceOutputType {
    //     match self.0.output_type {
    //         DataType::Metric => SourceOutputType::Metric,
    //         DataType::Log => SourceOutputType::Log,
    //         DataType::Any => SourceOutputType::Any,
    //     }
    // }
}

#[derive(Default)]
pub struct TopologyQuery;

#[Object]
impl TopologyQuery {
    async fn sources(&self) -> Vec<Source> {
        SOURCES.read().unwrap().iter().map(|s| s.clone()).collect()
    }
}

/// Update the 'global' configuration that will be consumed by topology queries
pub fn update_config(config: &Config) {
    *SOURCES.write().unwrap() = config
        .sources
        .iter()
        .map(|(name, source)| {
            Source(SourceData {
                name: name.to_owned(),
                output_type: source.output_type(),
            })
        })
        .collect()
}
