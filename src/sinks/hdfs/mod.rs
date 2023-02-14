//! `hdfs` sink.
//!
//! This sink will send it's output to HDFS.
//!
//! `hdfs` is an OpenDAL based services. This mod itself only provide
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

/// Configuration for the `hdfs` sink.
#[configurable_component(sink("hdfs"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]

pub struct HdfsConfig {
    /// A prefix/root to apply to all pathes.
    #[serde(default)]
    #[configurable(metadata(docs::templateable))]
    pub root: String,

    /// An HDFS cluster consists of a single NameNode, a master server that manages the file system namespace and regulates access to files by clients.
    ///
    /// For example:
    ///
    /// - `default`: visiting local fs.
    /// - `http://172.16.80.2:8090` visiting name node at `172.16.80.2`
    ///
    /// For more information: [HDFS Architecture](https://hadoop.apache.org/docs/r3.3.4/hadoop-project-dist/hadoop-hdfs/HdfsDesign.html#NameNode_and_DataNodes)
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
