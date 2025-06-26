use chrono::Utc;
use std::time::Duration;

use crate::sources::lldp::ffi::{LldpInterface, LldpNeighbor};
use crate::{
    config::{SourceConfig, SourceContext, SourceOutput},
    event::metric::{Metric, MetricKind, MetricTags, MetricValue},
};
use vector_lib::configurable::configurable_component;

mod ffi;
#[allow(improper_ctypes, unused_imports, non_camel_case_types, non_snake_case, non_upper_case_globals, dead_code)]
mod bindings;

/// Configuration for the `lldp` source.
#[configurable_component(source("lldp", "Collect lldp data."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct LldpMetricsConfig {
    /// interface data of lldp.
    #[serde(default = "default_interface_scrape_interval")]
    pub interface_scrape_secs: u64,

    /// link data of lldp.
    #[serde(default = "default_link_scrape_interval")]
    pub link_scrape_secs: u64,
}

fn default_interface_scrape_interval() -> u64 {
    30
}

fn default_link_scrape_interval() -> u64 {
    60
}
#[derive(Clone)]
pub struct Config {
    pub node_name: String,
    pub cluster: String,
}

impl_generate_config_from_default!(LldpMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "lldp")]
impl SourceConfig for LldpMetricsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let interface_scrape_secs = self.interface_scrape_secs;
        let link_scrape_secs = self.link_scrape_secs;
        let mut interface_out = cx.out.clone();
        let mut link_out = cx.out;
        let shutdown = cx.shutdown.clone();

        Ok(Box::pin(async move {
            let config = Config {
                node_name: std::env::var("NODE_NAME").unwrap_or_else(|_| "unknown-node".into()),
                cluster: std::env::var("CLUSTER_NAME").unwrap_or_else(|_| "unknown-cluster".into()),
            };

            let mut interface_interval =
                tokio::time::interval(Duration::from_secs(interface_scrape_secs));
            let mut link_interval = tokio::time::interval(Duration::from_secs(link_scrape_secs));

            loop {
                tokio::select! {
                    _ = interface_interval.tick() => {
                        match ffi::get_lldp_interfaces_async().await {
                            Ok(interfaces) => {
                                let interfaces_metrics = map_interfaces_to_metrics(interfaces, &config);
                                if let Err(_) = interface_out.send_batch(interfaces_metrics).await {
                                    return Err(());
                                }
                            }
                            Err(e) => warn!("LLDP interface error: {}", e),
                        }
                    }

                    _ = link_interval.tick() => {
                        match ffi::get_lldp_neighbors_async().await {
                            Ok(neighbors) => {
                                let (interfaces, links) = map_neighbors_to_interface_and_link(neighbors, &config);
                                if let Err(_) = link_out.send_batch(interfaces).await {
                                    return Err(());
                                }
                                if let Err(_) = link_out.send_batch(links).await {
                                    return Err(());
                                }
                            }
                            Err(e) => warn!("LLDP link error: {}", e),
                        }
                    }

                    _ = shutdown.clone() => {
                        info!("Shutting down LLDP source");
                        break;
                    }
                }
            }

            Ok(())
        }))
    }
    fn outputs(&self, _: vector_lib::config::LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

pub fn map_interfaces_to_metrics(interfaces: Vec<LldpInterface>, config: &Config) -> Vec<Metric> {
    let now = Utc::now();
    let mut metrics = Vec::new();

    for interface in interfaces {
        let mut tags = MetricTags::default();
        tags.insert("name".to_string(), interface.name.clone());
        tags.insert("device".to_string(), interface.device_name.clone());
        tags.insert("node_name".to_string(), config.node_name.clone());
        tags.insert("type".to_string(), "0".to_string());
        tags.insert("cluster".to_string(), config.cluster.clone());

        metrics.push(
            Metric::new(
                "lldp_interface",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 1.0 },
            )
            .with_timestamp(Some(now))
            .with_tags(Some(tags)),
        );
    }

    metrics
}

pub fn map_neighbors_to_interface_and_link(
    neighbors: Vec<LldpNeighbor>,
    config: &Config,
) -> (Vec<Metric>, Vec<Metric>) {
    let mut interface_metrics = Vec::new();
    let mut link_metrics = Vec::new();

    for neighbor in neighbors {
        let remote_type = if neighbor.remote_device.to_lowercase().contains("leaf") {
            1
        } else if neighbor.remote_device.to_lowercase().contains("spine") {
            2
        } else {
            3
        };

        let now = Utc::now();

        // switch interface
        let mut switch_tags = MetricTags::default();
        switch_tags.insert("name".to_string(), neighbor.remote_port.clone());
        switch_tags.insert("device".to_string(), neighbor.remote_device.clone());
        switch_tags.insert("node_name".to_string(), config.node_name.to_string());
        switch_tags.insert("type".to_string(), remote_type.to_string());
        switch_tags.insert("cluster".to_string(), config.cluster.clone());

        interface_metrics.push(
            Metric::new(
                "lldp_interface",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 1.0 },
            )
            .with_timestamp(Some(now))
            .with_tags(Some(switch_tags)),
        );

        let level = match remote_type {
            1 => 0,
            2 => 1,
            _ => 0,
        };

        // link
        let mut link_tags = MetricTags::default();
        link_tags.insert("from_name".to_string(), neighbor.local_interface.clone());
        link_tags.insert("from_device".to_string(), neighbor.local_device.clone());
        link_tags.insert("from_node".to_string(), config.node_name.clone());
        link_tags.insert("to_name".to_string(), neighbor.remote_port.clone());
        link_tags.insert("to_device".to_string(), neighbor.remote_device.to_string());
        link_tags.insert("cluster".to_string(), config.cluster.clone());
        link_tags.insert("level".to_string(), level.to_string());

        link_metrics.push(
            Metric::new(
                "lldp_link",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 1.0 },
            )
            .with_timestamp(Some(now))
            .with_tags(Some(link_tags)),
        );
    }

    (interface_metrics, link_metrics)
}
