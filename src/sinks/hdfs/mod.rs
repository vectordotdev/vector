//! `hdfs` sink.
//!
//! This sink will send it's output to hdfs.
//!
//! `hdfs` is an opendal based services. This mod itself only provide
//! config to build an [`OpendalSink`]. All real implement are powered by
//! [`OpendalSink`].

use super::opendal_common::*;
use super::Healthcheck;
use crate::config::{GenerateConfig, SinkConfig, SinkContext};
use opendal::services::Hdfs;
use opendal::Operator;
use vector_config::configurable_component;
use vector_core::{
    config::{AcknowledgementsConfig, Input},
    sink::VectorSink,
};

/// A sink that dumps its output to hdfs.
#[configurable_component(sink("hdfs"))]
#[derive(Clone, Debug)]
pub struct HdfsConfig {
    /// A prefix/root to apply to all pathes.
    #[serde(default)]
    #[configurable(metadata(docs::templateable))]
    pub root: String,
    #[configurable(derived)]
    #[serde(default)]
    pub name_node: String,
    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for HdfsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            root: "/tmp".to_string(),
            name_node: "default".to_string(),
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for HdfsConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        // Build OpenDAL Operator
        let mut builder = Hdfs::default();
        builder.root(&self.root);
        builder.name_node(&self.name_node);

        let op = Operator::create(builder)?.finish();

        let check_op = op.clone();
        let healthcheck = Box::pin(async move { Ok(check_op.check().await?) });
        let sink = VectorSink::from_event_streamsink(OpendalSink::new(op));

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}
