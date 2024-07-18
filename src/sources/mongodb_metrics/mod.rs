use std::time::{Duration, Instant};

use chrono::Utc;
use futures::{
    future::{join_all, try_join_all},
    StreamExt,
};
use mongodb::{
    bson::{self, doc, from_document, Bson, Document},
    error::Error as MongoError,
    options::ClientOptions,
    Client,
};
use serde_with::serde_as;
use snafu::{ResultExt, Snafu};
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::configurable::configurable_component;
use vector_lib::{metric_tags, ByteSizeOf, EstimatedJsonEncodedSizeOf};

use crate::{
    config::{SourceConfig, SourceContext, SourceOutput},
    event::metric::{Metric, MetricKind, MetricTags, MetricValue},
    internal_events::{
        CollectionCompleted, EndpointBytesReceived, MongoDbMetricsBsonParseError,
        MongoDbMetricsEventsReceived, MongoDbMetricsRequestError, StreamClosedError,
    },
};

mod types;
use types::{CommandBuildInfo, CommandIsMaster, CommandServerStatus, NodeType};
use vector_lib::config::LogNamespace;

macro_rules! tags {
    ($tags:expr) => { $tags.clone() };
    ($tags:expr, $($key:expr => $value:expr),*) => {
        {
            let mut tags = $tags.clone();
            $(
                tags.replace($key.into(), $value.to_string());
            )*
            tags
        }
    };
}

macro_rules! counter {
    ($value:expr) => {
        MetricValue::Counter {
            value: $value as f64,
        }
    };
}

macro_rules! gauge {
    ($value:expr) => {
        MetricValue::Gauge {
            value: $value as f64,
        }
    };
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("invalid endpoint: {}", source))]
    InvalidEndpoint { source: MongoError },
    #[snafu(display("invalid client options: {}", source))]
    InvalidClientOptions { source: MongoError },
}

#[derive(Debug)]
enum CollectError {
    Mongo(MongoError),
    Bson(bson::de::Error),
}

/// Configuration for the `mongodb_metrics` source.
#[serde_as]
#[configurable_component(source("mongodb_metrics", "Collect metrics from the MongoDB database."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct MongoDbMetricsConfig {
    /// A list of MongoDB instances to scrape.
    ///
    /// Each endpoint must be in the [Connection String URI Format](https://www.mongodb.com/docs/manual/reference/connection-string/).
    #[configurable(metadata(docs::examples = "mongodb://localhost:27017"))]
    endpoints: Vec<String>,

    /// The interval between scrapes, in seconds.
    #[serde(default = "default_scrape_interval_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    scrape_interval_secs: Duration,

    /// Overrides the default namespace for the metrics emitted by the source.
    ///
    /// If set to an empty string, no namespace is added to the metrics.
    ///
    /// By default, `mongodb` is used.
    #[serde(default = "default_namespace")]
    namespace: String,
}

#[derive(Debug)]
struct MongoDbMetrics {
    client: Client,
    endpoint: String,
    namespace: Option<String>,
    tags: MetricTags,
}

pub const fn default_scrape_interval_secs() -> Duration {
    Duration::from_secs(15)
}

pub fn default_namespace() -> String {
    "mongodb".to_string()
}

impl_generate_config_from_default!(MongoDbMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "mongodb_metrics")]
impl SourceConfig for MongoDbMetricsConfig {
    async fn build(&self, mut cx: SourceContext) -> crate::Result<super::Source> {
        let namespace = Some(self.namespace.clone()).filter(|namespace| !namespace.is_empty());

        let sources = try_join_all(
            self.endpoints
                .iter()
                .map(|endpoint| MongoDbMetrics::new(endpoint, namespace.clone())),
        )
        .await?;

        let duration = self.scrape_interval_secs;
        let shutdown = cx.shutdown;
        Ok(Box::pin(async move {
            let mut interval = IntervalStream::new(time::interval(duration)).take_until(shutdown);
            while interval.next().await.is_some() {
                let start = Instant::now();
                let metrics = join_all(sources.iter().map(|mongodb| mongodb.collect())).await;
                emit!(CollectionCompleted {
                    start,
                    end: Instant::now()
                });

                let metrics: Vec<Metric> = metrics.into_iter().flatten().collect();
                let count = metrics.len();

                if (cx.out.send_batch(metrics).await).is_err() {
                    emit!(StreamClosedError { count });
                    return Err(());
                }
            }

            Ok(())
        }))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

impl MongoDbMetrics {
    /// Works only with Standalone connection-string. Collect metrics only from specified instance.
    /// <https://docs.mongodb.com/manual/reference/connection-string/#standard-connection-string-format>
    async fn new(endpoint: &str, namespace: Option<String>) -> Result<MongoDbMetrics, BuildError> {
        let mut client_options = ClientOptions::parse(endpoint)
            .await
            .context(InvalidEndpointSnafu)?;
        client_options.direct_connection = Some(true);

        let endpoint = sanitize_endpoint(endpoint, &client_options);
        let tags = metric_tags!(
            "endpoint" => endpoint.clone(),
            "host" => client_options.hosts[0].to_string(),
        );

        Ok(Self {
            client: Client::with_options(client_options).context(InvalidClientOptionsSnafu)?,
            endpoint,
            namespace,
            tags,
        })
    }

    /// Finding node type for client with `isMaster` command.
    async fn get_node_type(&self) -> Result<NodeType, CollectError> {
        let doc = self
            .client
            .database("admin")
            .run_command(doc! { "isMaster": 1 }, None)
            .await
            .map_err(CollectError::Mongo)?;
        let msg: CommandIsMaster = from_document(doc).map_err(CollectError::Bson)?;

        Ok(if msg.set_name.is_some() || msg.hosts.is_some() {
            NodeType::Replset
        } else if msg.msg.map(|msg| msg == "isdbgrid").unwrap_or(false) {
            // Contains the value isdbgrid when isMaster returns from a mongos instance.
            // <https://docs.mongodb.com/manual/reference/command/isMaster/#isMaster.msg>
            // <https://docs.mongodb.com/manual/core/sharded-cluster-query-router/#confirm-connection-to-mongos-instances>
            NodeType::Mongos
        } else {
            NodeType::Mongod
        })
    }

    async fn get_build_info(&self) -> Result<CommandBuildInfo, CollectError> {
        let doc = self
            .client
            .database("admin")
            .run_command(doc! { "buildInfo": 1 }, None)
            .await
            .map_err(CollectError::Mongo)?;
        from_document(doc).map_err(CollectError::Bson)
    }

    async fn print_version(&self) -> Result<(), CollectError> {
        if tracing::level_enabled!(tracing::Level::DEBUG) {
            let node_type = self.get_node_type().await?;
            let build_info = self.get_build_info().await?;
            debug!(
                message = "Connected to server.", endpoint = %self.endpoint, node_type = ?node_type, server_version = ?serde_json::to_string(&build_info).unwrap()
            );
        }

        Ok(())
    }

    fn create_metric(&self, name: &str, value: MetricValue, tags: MetricTags) -> Metric {
        Metric::new(name, MetricKind::Absolute, value)
            .with_namespace(self.namespace.clone())
            .with_tags(Some(tags))
            .with_timestamp(Some(Utc::now()))
    }

    async fn collect(&self) -> Vec<Metric> {
        // `up` metric is `1` if collection is successful, otherwise `0`.
        let (up_value, mut metrics) = match self.collect_server_status().await {
            Ok(metrics) => (1.0, metrics),
            Err(error) => {
                match error {
                    CollectError::Mongo(error) => emit!(MongoDbMetricsRequestError {
                        error,
                        endpoint: &self.endpoint,
                    }),
                    CollectError::Bson(error) => emit!(MongoDbMetricsBsonParseError {
                        error,
                        endpoint: &self.endpoint,
                    }),
                }

                (0.0, vec![])
            }
        };

        metrics.push(self.create_metric("up", gauge!(up_value), tags!(self.tags)));

        emit!(MongoDbMetricsEventsReceived {
            byte_size: metrics.estimated_json_encoded_size_of(),
            count: metrics.len(),
            endpoint: &self.endpoint,
        });

        metrics
    }

    /// Collect metrics from `serverStatus` command.
    /// <https://docs.mongodb.com/manual/reference/command/serverStatus/>
    async fn collect_server_status(&self) -> Result<Vec<Metric>, CollectError> {
        self.print_version().await?;

        let mut metrics = vec![];

        let command = doc! { "serverStatus": 1, "opLatencies": { "histograms": true }};
        let db = self.client.database("admin");
        let doc = db
            .run_command(command, None)
            .await
            .map_err(CollectError::Mongo)?;
        let byte_size = document_size(&doc);
        emit!(EndpointBytesReceived {
            byte_size,
            protocol: "tcp",
            endpoint: &self.endpoint,
        });
        let status: CommandServerStatus = from_document(doc).map_err(CollectError::Bson)?;

        // asserts_total
        metrics.push(self.create_metric(
            "asserts_total",
            counter!(status.asserts.regular),
            tags!(self.tags, "type" => "regular"),
        ));
        metrics.push(self.create_metric(
            "asserts_total",
            counter!(status.asserts.warning),
            tags!(self.tags, "type" => "warning"),
        ));
        metrics.push(self.create_metric(
            "asserts_total",
            counter!(status.asserts.msg),
            tags!(self.tags, "type" => "msg"),
        ));
        metrics.push(self.create_metric(
            "asserts_total",
            counter!(status.asserts.user),
            tags!(self.tags, "type" => "user"),
        ));
        metrics.push(self.create_metric(
            "asserts_total",
            counter!(status.asserts.rollovers),
            tags!(self.tags, "type" => "rollovers"),
        ));

        // connections
        metrics.push(self.create_metric(
            "connections",
            counter!(status.connections.active),
            tags!(self.tags, "state" => "active"),
        ));
        metrics.push(self.create_metric(
            "connections",
            counter!(status.connections.available),
            tags!(self.tags, "state" => "available"),
        ));
        metrics.push(self.create_metric(
            "connections",
            counter!(status.connections.current),
            tags!(self.tags, "state" => "current"),
        ));

        // extra_info_*
        if let Some(value) = status.extra_info.heap_usage_bytes {
            metrics.push(self.create_metric(
                "extra_info_heap_usage_bytes",
                gauge!(value),
                tags!(self.tags),
            ));
        }
        metrics.push(self.create_metric(
            "extra_info_page_faults",
            gauge!(status.extra_info.page_faults),
            tags!(self.tags),
        ));

        // instance_*
        metrics.push(self.create_metric(
            "instance_local_time",
            gauge!(status.instance.local_time.timestamp_millis() / 1000),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "instance_uptime_estimate_seconds_total",
            gauge!(status.instance.uptime_estimate),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "instance_uptime_seconds_total",
            gauge!(status.instance.uptime),
            tags!(self.tags),
        ));

        // memory
        metrics.push(self.create_metric(
            "memory",
            gauge!(status.memory.resident),
            tags!(self.tags, "type" => "resident"),
        ));
        metrics.push(self.create_metric(
            "memory",
            gauge!(status.memory.r#virtual),
            tags!(self.tags, "type" => "virtual"),
        ));
        if let Some(value) = status.memory.mapped {
            metrics.push(self.create_metric(
                "memory",
                gauge!(value),
                tags!(self.tags, "type" => "mapped"),
            ))
        }
        if let Some(value) = status.memory.mapped_with_journal {
            metrics.push(self.create_metric(
                "memory",
                gauge!(value),
                tags!(self.tags, "type" => "mapped_with_journal"),
            ))
        }

        // mongod_global_lock_*
        metrics.push(self.create_metric(
            "mongod_global_lock_total_time_seconds",
            counter!(status.global_lock.total_time),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_global_lock_active_clients",
            gauge!(status.global_lock.active_clients.total),
            tags!(self.tags, "type" => "total"),
        ));
        metrics.push(self.create_metric(
            "mongod_global_lock_active_clients",
            gauge!(status.global_lock.active_clients.readers),
            tags!(self.tags, "type" => "readers"),
        ));
        metrics.push(self.create_metric(
            "mongod_global_lock_active_clients",
            gauge!(status.global_lock.active_clients.writers),
            tags!(self.tags, "type" => "writers"),
        ));
        metrics.push(self.create_metric(
            "mongod_global_lock_current_queue",
            gauge!(status.global_lock.current_queue.total),
            tags!(self.tags, "type" => "total"),
        ));
        metrics.push(self.create_metric(
            "mongod_global_lock_current_queue",
            gauge!(status.global_lock.current_queue.readers),
            tags!(self.tags, "type" => "readers"),
        ));
        metrics.push(self.create_metric(
            "mongod_global_lock_current_queue",
            gauge!(status.global_lock.current_queue.writers),
            tags!(self.tags, "type" => "writers"),
        ));

        // mongod_locks_time_*
        for (r#type, lock) in status.locks {
            if let Some(modes) = lock.time_acquiring_micros {
                if let Some(value) = modes.read {
                    metrics.push(self.create_metric(
                        "mongod_locks_time_acquiring_global_seconds_total",
                        counter!(value),
                        tags!(self.tags, "type" => &r#type, "mode" => "read"),
                    ));
                }
                if let Some(value) = modes.write {
                    metrics.push(self.create_metric(
                        "mongod_locks_time_acquiring_global_seconds_total",
                        counter!(value),
                        tags!(self.tags, "type" => &r#type, "mode" => "write"),
                    ));
                }
            }
        }

        // mongod_metrics_cursor_*
        metrics.push(self.create_metric(
            "mongod_metrics_cursor_timed_out_total",
            counter!(status.metrics.cursor.timed_out),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_cursor_open",
            gauge!(status.metrics.cursor.open.no_timeout),
            tags!(self.tags, "state" => "no_timeout"),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_cursor_open",
            gauge!(status.metrics.cursor.open.pinned),
            tags!(self.tags, "state" => "pinned"),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_cursor_open",
            gauge!(status.metrics.cursor.open.total),
            tags!(self.tags, "state" => "total"),
        ));

        // mongod_metrics_document_total
        metrics.push(self.create_metric(
            "mongod_metrics_document_total",
            counter!(status.metrics.document.deleted),
            tags!(self.tags, "state" => "deleted"),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_document_total",
            counter!(status.metrics.document.inserted),
            tags!(self.tags, "state" => "inserted"),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_document_total",
            counter!(status.metrics.document.returned),
            tags!(self.tags, "state" => "returned"),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_document_total",
            counter!(status.metrics.document.updated),
            tags!(self.tags, "state" => "updated"),
        ));

        // mongod_metrics_get_last_error_*
        metrics.push(self.create_metric(
            "mongod_metrics_get_last_error_wtime_num",
            gauge!(status.metrics.get_last_error.wtime.num),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_get_last_error_wtime_seconds_total",
            counter!(status.metrics.get_last_error.wtime.total_millis / 1000),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_get_last_error_wtimeouts_total",
            counter!(status.metrics.get_last_error.wtimeouts),
            tags!(self.tags),
        ));

        // mongod_metrics_operation_total
        metrics.push(self.create_metric(
            "mongod_metrics_operation_total",
            counter!(status.metrics.operation.scan_and_order),
            tags!(self.tags, "type" => "scan_and_order"),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_operation_total",
            counter!(status.metrics.operation.write_conflicts),
            tags!(self.tags, "type" => "write_conflicts"),
        ));

        // mongod_metrics_query_executor_total
        metrics.push(self.create_metric(
            "mongod_metrics_query_executor_total",
            counter!(status.metrics.query_executor.scanned),
            tags!(self.tags, "state" => "scanned"),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_query_executor_total",
            counter!(status.metrics.query_executor.scanned_objects),
            tags!(self.tags, "state" => "scanned_objects"),
        ));
        if let Some(doc) = status.metrics.query_executor.collection_scans {
            metrics.push(self.create_metric(
                "mongod_metrics_query_executor_total",
                counter!(doc.total),
                tags!(self.tags, "state" => "collection_scans"),
            ));
        }

        // mongod_metrics_record_moves_total
        if let Some(record) = status.metrics.record {
            metrics.push(self.create_metric(
                "mongod_metrics_record_moves_total",
                counter!(record.moves),
                tags!(self.tags),
            ));
        }

        // mongod_metrics_repl_apply_
        metrics.push(self.create_metric(
            "mongod_metrics_repl_apply_batches_num_total",
            counter!(status.metrics.repl.apply.batches.num),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_repl_apply_batches_seconds_total",
            counter!(status.metrics.repl.apply.batches.total_millis / 1000),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_repl_apply_ops_total",
            counter!(status.metrics.repl.apply.ops),
            tags!(self.tags),
        ));

        // mongod_metrics_repl_buffer_*
        metrics.push(self.create_metric(
            "mongod_metrics_repl_buffer_count",
            counter!(status.metrics.repl.buffer.count),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_repl_buffer_max_size_bytes_total",
            counter!(status.metrics.repl.buffer.max_size_bytes),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_repl_buffer_size_bytes",
            counter!(status.metrics.repl.buffer.size_bytes),
            tags!(self.tags),
        ));

        // mongod_metrics_repl_executor_*
        metrics.push(self.create_metric(
            "mongod_metrics_repl_executor_queue",
            gauge!(status.metrics.repl.executor.queues.network_in_progress),
            tags!(self.tags, "type" => "network_in_progress"),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_repl_executor_queue",
            gauge!(status.metrics.repl.executor.queues.sleepers),
            tags!(self.tags, "type" => "sleepers"),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_repl_executor_unsignaled_events",
            gauge!(status.metrics.repl.executor.unsignaled_events),
            tags!(self.tags),
        ));

        // mongod_metrics_repl_network_*
        metrics.push(self.create_metric(
            "mongod_metrics_repl_network_bytes_total",
            counter!(status.metrics.repl.network.bytes),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_repl_network_getmores_num_total",
            counter!(status.metrics.repl.network.getmores.num),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_repl_network_getmores_seconds_total",
            counter!(status.metrics.repl.network.getmores.total_millis / 1000),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_repl_network_ops_total",
            counter!(status.metrics.repl.network.ops),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_repl_network_readers_created_total",
            counter!(status.metrics.repl.network.readers_created),
            tags!(self.tags),
        ));

        // mongod_metrics_ttl_*
        metrics.push(self.create_metric(
            "mongod_metrics_ttl_deleted_documents_total",
            counter!(status.metrics.ttl.deleted_documents),
            tags!(self.tags),
        ));
        metrics.push(self.create_metric(
            "mongod_metrics_ttl_passes_total",
            counter!(status.metrics.ttl.passes),
            tags!(self.tags),
        ));

        // mongod_op_latencies_*
        for (r#type, stat) in status.op_latencies {
            for bucket in stat.histogram {
                metrics.push(self.create_metric(
                    "mongod_op_latencies_histogram",
                    gauge!(bucket.count),
                    tags!(self.tags, "type" => &r#type, "micros" => bucket.micros.to_string()),
                ));
            }
            metrics.push(self.create_metric(
                "mongod_op_latencies_latency",
                gauge!(stat.latency),
                tags!(self.tags, "type" => &r#type),
            ));
            metrics.push(self.create_metric(
                "mongod_op_latencies_ops_total",
                gauge!(stat.ops),
                tags!(self.tags, "type" => &r#type),
            ));
        }

        // mongod_storage_engine
        metrics.push(self.create_metric(
            "mongod_storage_engine",
            gauge!(1),
            tags!(self.tags, "engine" => status.storage_engine.name),
        ));

        // mongod_wiredtiger_*
        if let Some(stat) = status.wired_tiger {
            // mongod_wiredtiger_blockmanager_blocks_total
            metrics.push(self.create_metric(
                "mongod_wiredtiger_blockmanager_blocks_total",
                counter!(stat.block_manager.blocks_read),
                tags!(self.tags, "type" => "blocks_read"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_blockmanager_blocks_total",
                counter!(stat.block_manager.mapped_blocks_read),
                tags!(self.tags, "type" => "blocks_read_mapped"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_blockmanager_blocks_total",
                counter!(stat.block_manager.blocks_pre_loaded),
                tags!(self.tags, "type" => "blocks_pre_loaded"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_blockmanager_blocks_total",
                counter!(stat.block_manager.blocks_written),
                tags!(self.tags, "type" => "blocks_written"),
            ));

            // mongod_wiredtiger_blockmanager_bytes_total
            metrics.push(self.create_metric(
                "mongod_wiredtiger_blockmanager_bytes_total",
                counter!(stat.block_manager.bytes_read),
                tags!(self.tags, "type" => "bytes_read"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_blockmanager_bytes_total",
                counter!(stat.block_manager.mapped_bytes_read),
                tags!(self.tags, "type" => "bytes_read_mapped"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_blockmanager_bytes_total",
                counter!(stat.block_manager.bytes_written),
                tags!(self.tags, "type" => "bytes_written"),
            ));

            // mongod_wiredtiger_cache_bytes
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_bytes",
                gauge!(stat.cache.bytes_total),
                tags!(self.tags, "type" => "total"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_bytes",
                gauge!(stat.cache.bytes_dirty),
                tags!(self.tags, "type" => "dirty"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_bytes",
                gauge!(stat.cache.bytes_internal_pages),
                tags!(self.tags, "type" => "internal_pages"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_bytes",
                gauge!(stat.cache.bytes_leaf_pages),
                tags!(self.tags, "type" => "leaf_pages"),
            ));

            // mongod_wiredtiger_cache_bytes_total
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_bytes_total",
                counter!(stat.cache.pages_read_into),
                tags!(self.tags, "type" => "read"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_bytes_total",
                counter!(stat.cache.pages_written_from),
                tags!(self.tags, "type" => "written"),
            ));

            // mongod_wiredtiger_cache_evicted_total
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_evicted_total",
                counter!(stat.cache.evicted_modified),
                tags!(self.tags, "type" => "modified"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_evicted_total",
                counter!(stat.cache.evicted_unmodified),
                tags!(self.tags, "type" => "unmodified"),
            ));

            // mongod_wiredtiger_cache_max_bytes
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_max_bytes",
                gauge!(stat.cache.max_bytes),
                tags!(self.tags),
            ));

            // mongod_wiredtiger_cache_overhead_percent
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_overhead_percent",
                gauge!(stat.cache.percent_overhead),
                tags!(self.tags),
            ));

            // mongod_wiredtiger_cache_pages
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_pages",
                gauge!(stat.cache.pages_total),
                tags!(self.tags, "type" => "total"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_pages",
                gauge!(stat.cache.pages_dirty),
                tags!(self.tags, "type" => "dirty"),
            ));

            // mongod_wiredtiger_cache_pages_total
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_pages_total",
                counter!(stat.cache.pages_read_into),
                tags!(self.tags, "type" => "read"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_cache_pages_total",
                counter!(stat.cache.pages_written_from),
                tags!(self.tags, "type" => "write"),
            ));

            // mongod_wiredtiger_concurrent_transactions_*
            metrics.push(self.create_metric(
                "mongod_wiredtiger_concurrent_transactions_available_tickets",
                gauge!(stat.concurrent_transactions.read.available),
                tags!(self.tags, "type" => "read"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_concurrent_transactions_available_tickets",
                gauge!(stat.concurrent_transactions.write.available),
                tags!(self.tags, "type" => "write"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_concurrent_transactions_out_tickets",
                gauge!(stat.concurrent_transactions.read.out),
                tags!(self.tags, "type" => "read"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_concurrent_transactions_out_tickets",
                gauge!(stat.concurrent_transactions.write.out),
                tags!(self.tags, "type" => "write"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_concurrent_transactions_total_tickets",
                gauge!(stat.concurrent_transactions.read.total_tickets),
                tags!(self.tags, "type" => "read"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_concurrent_transactions_total_tickets",
                gauge!(stat.concurrent_transactions.write.total_tickets),
                tags!(self.tags, "type" => "write"),
            ));

            // mongod_wiredtiger_log_*
            metrics.push(self.create_metric(
                "mongod_wiredtiger_log_bytes_total",
                counter!(stat.log.bytes_payload_data),
                tags!(self.tags, "type" => "payload"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_log_bytes_total",
                counter!(stat.log.bytes_written),
                tags!(self.tags, "type" => "written"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_log_operations_total",
                counter!(stat.log.log_writes),
                tags!(self.tags, "type" => "write"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_log_operations_total",
                counter!(stat.log.log_scans),
                tags!(self.tags, "type" => "scan"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_log_operations_total",
                counter!(stat.log.log_scans_double),
                tags!(self.tags, "type" => "scan_double"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_log_operations_total",
                counter!(stat.log.log_syncs),
                tags!(self.tags, "type" => "sync"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_log_operations_total",
                counter!(stat.log.log_sync_dirs),
                tags!(self.tags, "type" => "sync_dir"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_log_operations_total",
                counter!(stat.log.log_flushes),
                tags!(self.tags, "type" => "flush"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_log_records_scanned_total",
                counter!(stat.log.records_compressed),
                tags!(self.tags, "type" => "compressed"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_log_records_scanned_total",
                counter!(stat.log.records_uncompressed),
                tags!(self.tags, "type" => "uncompressed"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_log_records_total",
                counter!(stat.log.records_processed_log_scan),
                tags!(self.tags),
            ));

            // mongod_wiredtiger_session_open_sessions
            metrics.push(self.create_metric(
                "mongod_wiredtiger_session_open_sessions",
                gauge!(stat.session.sessions),
                tags!(self.tags),
            ));

            // mongod_wiredtiger_transactions_*
            metrics.push(self.create_metric(
                "mongod_wiredtiger_transactions_checkpoint_seconds",
                gauge!(stat.transaction.checkpoint_min_ms / 1000),
                tags!(self.tags, "type" => "min"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_transactions_checkpoint_seconds",
                gauge!(stat.transaction.checkpoint_max_ms / 1000),
                tags!(self.tags, "type" => "max"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_transactions_checkpoint_seconds_total",
                counter!(stat.transaction.checkpoint_total_ms / 1000),
                tags!(self.tags),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_transactions_running_checkpoints",
                gauge!(stat.transaction.checkpoints_running),
                tags!(self.tags),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_transactions_total",
                counter!(stat.transaction.begins),
                tags!(self.tags, "type" => "begins"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_transactions_total",
                counter!(stat.transaction.checkpoints),
                tags!(self.tags, "type" => "checkpoints"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_transactions_total",
                counter!(stat.transaction.committed),
                tags!(self.tags, "type" => "committed"),
            ));
            metrics.push(self.create_metric(
                "mongod_wiredtiger_transactions_total",
                counter!(stat.transaction.rolled_back),
                tags!(self.tags, "type" => "rolledback"),
            ));
        }

        // network_*
        metrics.push(self.create_metric(
            "network_bytes_total",
            counter!(status.network.bytes_in),
            tags!(self.tags, "state" => "bytes_in"),
        ));
        metrics.push(self.create_metric(
            "network_bytes_total",
            counter!(status.network.bytes_out),
            tags!(self.tags, "state" => "bytes_out"),
        ));
        metrics.push(self.create_metric(
            "network_metrics_num_requests_total",
            counter!(status.network.num_requests),
            tags!(self.tags),
        ));

        // op_counters_repl_total
        for (r#type, value) in status.opcounters {
            metrics.push(self.create_metric(
                "op_counters_repl_total",
                counter!(value),
                tags!(self.tags, "type" => r#type),
            ));
        }

        // op_counters_total
        for (r#type, value) in status.opcounters_repl {
            metrics.push(self.create_metric(
                "op_counters_total",
                counter!(value),
                tags!(self.tags, "type" => r#type),
            ));
        }

        Ok(metrics)
    }
}

fn bson_size(value: &Bson) -> usize {
    match value {
        Bson::Double(value) => value.size_of(),
        Bson::String(value) => value.size_of(),
        Bson::Array(value) => value.iter().map(bson_size).sum(),
        Bson::Document(value) => document_size(value),
        Bson::Boolean(_) => std::mem::size_of::<bool>(),
        Bson::RegularExpression(value) => value.pattern.size_of(),
        Bson::JavaScriptCode(value) => value.size_of(),
        Bson::JavaScriptCodeWithScope(value) => value.code.size_of() + document_size(&value.scope),
        Bson::Int32(value) => value.size_of(),
        Bson::Int64(value) => value.size_of(),
        Bson::Timestamp(value) => value.time.size_of() + value.increment.size_of(),
        Bson::Binary(value) => value.bytes.size_of(),
        Bson::ObjectId(value) => value.bytes().size_of(),
        Bson::DateTime(_) => std::mem::size_of::<i64>(),
        Bson::Symbol(value) => value.size_of(),
        Bson::Decimal128(value) => value.bytes().size_of(),
        Bson::DbPointer(_) => {
            // DbPointer parts are not public and cannot be evaluated
            0
        }
        Bson::Null | Bson::Undefined | Bson::MaxKey | Bson::MinKey => 0,
    }
}

fn document_size(doc: &Document) -> usize {
    doc.into_iter()
        .map(|(key, value)| key.size_of() + bson_size(value))
        .sum()
}

/// Remove credentials from endpoint.
/// URI components: <https://docs.mongodb.com/manual/reference/connection-string/#components>
/// It's not possible to use [url::Url](https://docs.rs/url/2.1.1/url/struct.Url.html) because connection string can have multiple hosts.
/// Would be nice to serialize [ClientOptions](https://docs.rs/mongodb/1.1.1/mongodb/options/struct.ClientOptions.html) to String, but it's not supported.
/// `endpoint` argument would not be required, but field `original_uri` in `ClientOptions` is private.
/// `.unwrap()` in function is safe because endpoint was already verified by `ClientOptions`.
/// Based on ClientOptions::parse_uri -- <https://github.com/mongodb/mongo-rust-driver/blob/09e1193f93dcd850ebebb7fb82f6ab786fd85de1/src/client/options/mod.rs#L708>
fn sanitize_endpoint(endpoint: &str, options: &ClientOptions) -> String {
    let mut endpoint = endpoint.to_owned();
    if options.credential.is_some() {
        let start = endpoint.find("://").unwrap() + 3;

        // Split `username:password@host[:port]` and `/defaultauthdb?<options>`
        let pre_slash = match endpoint[start..].find('/') {
            Some(index) => {
                let mut segments = endpoint[start..].split_at(index);
                // If we have databases and options
                if segments.1.len() > 1 {
                    let lstart = start + segments.0.len() + 1;
                    let post_slash = &segments.1[1..];
                    // Split `/defaultauthdb` and `?<options>`
                    if let Some(index) = post_slash.find('?') {
                        let segments = post_slash.split_at(index);
                        // If we have options
                        if segments.1.len() > 1 {
                            // Remove authentication options
                            let options = segments.1[1..]
                                .split('&')
                                .filter(|pair| {
                                    let (key, _) = pair.split_at(pair.find('=').unwrap());
                                    !matches!(
                                        key.to_lowercase().as_str(),
                                        "authsource" | "authmechanism" | "authmechanismproperties"
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("&");

                            // Update options in endpoint
                            endpoint = format!(
                                "{}{}",
                                &endpoint[..lstart + segments.0.len() + 1],
                                &options
                            );
                        }
                    }
                    segments = endpoint[start..].split_at(index);
                }
                segments.0
            }
            None => &endpoint[start..],
        };

        // Remove `username:password@`
        let end = pre_slash.rfind('@').unwrap() + 1;
        endpoint = format!("{}{}", &endpoint[0..start], &endpoint[start + end..]);
    }
    endpoint
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MongoDbMetricsConfig>();
    }

    #[tokio::test]
    async fn sanitize_endpoint_test() {
        let endpoint = "mongodb://myDBReader:D1fficultP%40ssw0rd@mongos0.example.com:27017,mongos1.example.com:27017,mongos2.example.com:27017/?authSource=admin&tls=true";
        let client_options = ClientOptions::parse(endpoint).await.unwrap();
        let endpoint = sanitize_endpoint(endpoint, &client_options);
        assert_eq!(&endpoint, "mongodb://mongos0.example.com:27017,mongos1.example.com:27017,mongos2.example.com:27017/?tls=true");
    }
}

#[cfg(all(test, feature = "mongodb_metrics-integration-tests"))]
mod integration_tests {
    use futures::StreamExt;
    use tokio::time::{timeout, Duration};

    use super::*;
    use crate::{
        test_util::{
            components::{assert_source_compliance, PULL_SOURCE_TAGS},
            trace_init,
        },
        SourceSender,
    };

    fn primary_mongo_address() -> String {
        std::env::var("PRIMARY_MONGODB_ADDRESS")
            .unwrap_or_else(|_| "mongodb://localhost:27017".into())
    }

    fn secondary_mongo_address() -> String {
        std::env::var("SECONDARY_MONGODB_ADDRESS")
            .unwrap_or_else(|_| "mongodb://localhost:27019".into())
    }

    fn remove_creds(address: &str) -> String {
        let mut url = url::Url::parse(address).unwrap();
        url.set_password(None).unwrap();
        url.set_username("").unwrap();
        url.to_string()
    }

    async fn test_instance(endpoint: String) {
        assert_source_compliance(&PULL_SOURCE_TAGS, async {
            let host = ClientOptions::parse(endpoint.as_str()).await.unwrap().hosts[0].to_string();
            let namespace = "vector_mongodb";

            let (sender, mut recv) = SourceSender::new_test();

            let endpoints = vec![endpoint.clone()];
            tokio::spawn(async move {
                MongoDbMetricsConfig {
                    endpoints,
                    scrape_interval_secs: Duration::from_secs(15),
                    namespace: namespace.to_owned(),
                }
                .build(SourceContext::new_test(sender, None))
                .await
                .unwrap()
                .await
                .unwrap()
            });

            // TODO: We should have a simpler/cleaner method for this sort of collection, where we're essentially waiting
            // for a burst of events, and want to debounce ourselves in terms of stopping collection once all events in the
            // burst have been collected. This code here isn't bad or anything... I've just noticed now that we do it in a
            // few places, and we could solve it in a cleaner way, most likely.
            let event = timeout(Duration::from_secs(30), recv.next())
                .await
                .expect("fetch metrics timeout")
                .expect("failed to get metrics from a stream");
            let mut events = vec![event];
            loop {
                match timeout(Duration::from_millis(10), recv.next()).await {
                    Ok(Some(event)) => events.push(event),
                    Ok(None) => break,
                    Err(_) => break,
                }
            }

            let clean_endpoint = remove_creds(&endpoint);

            assert!(events.len() > 100);
            for event in events {
                let metric = event.into_metric();
                // validate namespace
                assert!(metric.namespace() == Some(namespace));
                // validate timestamp
                let timestamp = metric.timestamp().expect("existed timestamp");
                assert!((timestamp - Utc::now()).num_seconds() < 1);
                // validate basic tags
                let tags = metric.tags().expect("existed tags");
                assert_eq!(tags.get("endpoint"), Some(&clean_endpoint[..]));
                assert_eq!(tags.get("host"), Some(&host[..]));
            }
        })
        .await;
    }

    #[tokio::test]
    async fn fetch_metrics_mongod() {
        trace_init();
        test_instance(primary_mongo_address()).await;
    }

    // TODO
    // #[tokio::test]
    // async fn fetch_metrics_mongos() {
    //     trace_init();
    //     test_instance("mongodb://localhost:27018").await;
    // }

    #[tokio::test]
    async fn fetch_metrics_replset() {
        trace_init();
        test_instance(secondary_mongo_address()).await;
    }
}
