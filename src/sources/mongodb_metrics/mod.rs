use crate::{
    config::{self, GlobalOptions, SourceConfig, SourceDescription},
    event::metric::{Metric, MetricKind, MetricValue},
    internal_events::{
        MongoDBMetricsBsonParseError, MongoDBMetricsCollectCompleted, MongoDBMetricsRequestError,
    },
    shutdown::ShutdownSignal,
    Event, Pipeline,
};
use chrono::Utc;
use futures::{
    future::{join_all, try_join_all},
    stream, SinkExt, StreamExt,
};
use mongodb::{
    bson::{self, doc, from_document},
    error::Error as MongoError,
    options::ClientOptions,
    Client,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{collections::BTreeMap, future::ready, time::Instant};
use tokio::time;

mod types;
use types::{CommandBuildInfo, CommandIsMaster, CommandServerStatus, NodeType};

macro_rules! tags {
    ($tags:expr) => { $tags.clone() };
    ($tags:expr, $($key:expr => $value:expr),*) => {
        {
            let mut tags = $tags.clone();
            $(
                tags.insert($key.into(), $value.into());
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
    #[snafu(display("failed to execute `isMaster` command: {}", source))]
    CommandIsMasterMongoError { source: MongoError },
    #[snafu(display("failed to parse `isMaster` response: {}", source))]
    CommandIsMasterParseError { source: bson::de::Error },
    #[snafu(display("failed to execute `buildInfo` command: {}", source))]
    CommandBuildInfoMongoError { source: MongoError },
    #[snafu(display("failed to parse `buildInfo` response: {}", source))]
    CommandBuildInfoParseError { source: bson::de::Error },
}

#[derive(Debug)]
enum CollectError {
    Mongo(MongoError),
    Bson(bson::de::Error),
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
struct MongoDBMetricsConfig {
    endpoints: Vec<String>,
    #[serde(default = "default_scrape_interval_secs")]
    scrape_interval_secs: u64,
    #[serde(default = "default_namespace")]
    namespace: String,
}

#[derive(Debug)]
struct MongoDBMetrics {
    client: Client,
    endpoint: String,
    namespace: Option<String>,
    tags: BTreeMap<String, String>,
}

pub fn default_scrape_interval_secs() -> u64 {
    15
}

pub fn default_namespace() -> String {
    "mongodb".to_string()
}

inventory::submit! {
    SourceDescription::new::<MongoDBMetricsConfig>("mongodb_metrics")
}

impl_generate_config_from_default!(MongoDBMetricsConfig);

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
        let namespace = Some(self.namespace.clone()).filter(|namespace| !namespace.is_empty());

        let sources = try_join_all(
            self.endpoints
                .iter()
                .map(|endpoint| MongoDBMetrics::new(endpoint, namespace.clone())),
        )
        .await?;

        let mut out =
            out.sink_map_err(|error| error!(message = "Error sending mongodb metrics.", %error));

        let duration = time::Duration::from_secs(self.scrape_interval_secs);
        Ok(Box::pin(async move {
            let mut interval = time::interval(duration).take_until(shutdown);
            while interval.next().await.is_some() {
                let start = Instant::now();
                let metrics = join_all(sources.iter().map(|mongodb| mongodb.collect())).await;
                emit!(MongoDBMetricsCollectCompleted {
                    start,
                    end: Instant::now()
                });

                let mut stream = stream::iter(metrics).flatten().map(Event::Metric).map(Ok);
                out.send_all(&mut stream).await?;
            }

            Ok(())
        }))
    }

    fn output_type(&self) -> config::DataType {
        config::DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "mongodb_metrics"
    }
}

impl MongoDBMetrics {
    /// Works only with Standalone connection-string. Collect metrics only from specified instance.
    /// https://docs.mongodb.com/manual/reference/connection-string/#standard-connection-string-format
    async fn new(endpoint: &str, namespace: Option<String>) -> Result<MongoDBMetrics, BuildError> {
        let mut tags: BTreeMap<String, String> = BTreeMap::new();

        let mut client_options = ClientOptions::parse(endpoint)
            .await
            .context(InvalidEndpoint)?;
        client_options.direct_connection = Some(true);

        let endpoint = Self::sanitize_endpoint(endpoint, &client_options);
        tags.insert("endpoint".into(), endpoint.clone());
        tags.insert("host".into(), client_options.hosts[0].to_string());

        let client = Client::with_options(client_options).context(InvalidClientOptions)?;

        let node_type = Self::get_node_type(&client).await?;
        let build_info = Self::get_build_info(&client).await?;
        debug!(
            message = "Connected to server.", endpoint = %endpoint, node_type = ?node_type, server_version = ?serde_json::to_string(&build_info).unwrap()
        );

        Ok(Self {
            client,
            endpoint,
            namespace,
            tags,
        })
    }

    /// Remove credentials from endpoint.
    /// URI components: https://docs.mongodb.com/manual/reference/connection-string/#components
    /// It's not possible to use [url::Url](https://docs.rs/url/2.1.1/url/struct.Url.html) because connection string can have multiple hosts.
    /// Would be nice to serialize [ClientOptions][https://docs.rs/mongodb/1.1.1/mongodb/options/struct.ClientOptions.html] to String, but it's not supported.
    /// `endpoint` argument would not be required, but field `original_uri` in `ClieotnOptions` is private.
    /// `.unwrap()` in function is safe because endpoint was already verified by `ClientOptions`.
    /// Based on ClientOptions::parse_uri -- https://github.com/mongodb/mongo-rust-driver/blob/09e1193f93dcd850ebebb7fb82f6ab786fd85de1/src/client/options/mod.rs#L708
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
                                            "authsource"
                                                | "authmechanism"
                                                | "authmechanismproperties"
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

    /// Finding node type for client with `isMaster` command.
    async fn get_node_type(client: &Client) -> Result<NodeType, BuildError> {
        let doc = client
            .database("admin")
            .run_command(doc! { "isMaster": 1 }, None)
            .await
            .context(CommandIsMasterMongoError)?;
        let msg: CommandIsMaster = from_document(doc).context(CommandIsMasterParseError)?;

        Ok(if msg.set_name.is_some() || msg.hosts.is_some() {
            NodeType::Replset
        } else if msg.msg.map(|msg| msg == "isdbgrid").unwrap_or(false) {
            // Contains the value isdbgrid when isMaster returns from a mongos instance.
            // https://docs.mongodb.com/manual/reference/command/isMaster/#isMaster.msg
            // https://docs.mongodb.com/manual/core/sharded-cluster-query-router/#confirm-connection-to-mongos-instances
            NodeType::Mongos
        } else {
            NodeType::Mongod
        })
    }

    async fn get_build_info(client: &Client) -> Result<CommandBuildInfo, BuildError> {
        let doc = client
            .database("admin")
            .run_command(doc! { "buildInfo": 1 }, None)
            .await
            .context(CommandBuildInfoMongoError)?;
        from_document(doc).context(CommandBuildInfoParseError)
    }

    fn create_metric(
        &self,
        name: &str,
        value: MetricValue,
        tags: BTreeMap<String, String>,
    ) -> Metric {
        Metric::new(
            name.into(),
            self.namespace.clone(),
            Some(Utc::now()),
            Some(tags),
            MetricKind::Absolute,
            value,
        )
    }

    async fn collect(&self) -> stream::BoxStream<'static, Metric> {
        // `up` metric is `1` if collection is successful, otherwise `0`.
        let (up_value, metrics) = match self.collect_server_status().await {
            Ok(metrics) => (1.0, metrics),
            Err(error) => {
                match error {
                    CollectError::Mongo(error) => emit!(MongoDBMetricsRequestError {
                        error,
                        endpoint: &self.endpoint,
                    }),
                    CollectError::Bson(error) => emit!(MongoDBMetricsBsonParseError {
                        error,
                        endpoint: &self.endpoint,
                    }),
                }

                (0.0, vec![])
            }
        };

        stream::once(ready(self.create_metric(
            "up",
            gauge!(up_value),
            tags!(self.tags),
        )))
        .chain(stream::iter(metrics))
        .boxed()
    }

    /// Collect metrics from `serverStatus` command.
    /// https://docs.mongodb.com/manual/reference/command/serverStatus/
    async fn collect_server_status(&self) -> Result<Vec<Metric>, CollectError> {
        let mut metrics = vec![];

        let command = doc! { "serverStatus": 1, "opLatencies": { "histograms": true }};
        let db = self.client.database("admin");
        let doc = db
            .run_command(command, None)
            .await
            .map_err(CollectError::Mongo)?;
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
            gauge!(status.instance.local_time.timestamp()),
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
        metrics.push(self.create_metric(
            "mongod_metrics_record_moves_total",
            counter!(status.metrics.record.moves),
            tags!(self.tags),
        ));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MongoDBMetricsConfig>();
    }

    #[tokio::test]
    async fn sanitize_endpoint() {
        let endpoint = "mongodb://myDBReader:D1fficultP%40ssw0rd@mongos0.example.com:27017,mongos1.example.com:27017,mongos2.example.com:27017/?authSource=admin&tls=true";
        let client_options = ClientOptions::parse(endpoint).await.unwrap();
        let endpoint = MongoDBMetrics::sanitize_endpoint(endpoint, &client_options);
        assert_eq!(&endpoint, "mongodb://mongos0.example.com:27017,mongos1.example.com:27017,mongos2.example.com:27017/?tls=true");
    }
}

#[cfg(all(test, feature = "mongodb_metrics-integration-tests"))]
mod integration_tests {
    use super::*;
    use crate::{test_util::trace_init, Pipeline};
    use futures::StreamExt;
    use tokio::time::{timeout, Duration};

    async fn test_instance(endpoint: &'static str) {
        let host = ClientOptions::parse(endpoint).await.unwrap().hosts[0].to_string();
        let namespace = "vector_mongodb";

        let (sender, mut recv) = Pipeline::new_test();

        tokio::spawn(async move {
            MongoDBMetricsConfig {
                endpoints: vec![endpoint.to_owned()],
                scrape_interval_secs: 15,
                namespace: namespace.to_owned(),
            }
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                sender,
            )
            .await
            .unwrap()
            .await
            .unwrap()
        });

        let event = timeout(Duration::from_secs(3), recv.next())
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

        assert!(events.len() > 100);
        for event in events {
            let metric = event.into_metric();
            // validate namespace
            assert!(metric.namespace() == Some(namespace));
            // validate timestamp
            let timestamp = metric.data.timestamp.expect("existed timestamp");
            assert!((timestamp - Utc::now()).num_seconds() < 1);
            // validate basic tags
            let tags = metric.tags().expect("existed tags");
            assert_eq!(tags.get("endpoint").map(String::as_ref), Some(endpoint));
            assert_eq!(tags.get("host"), Some(&host));
        }
    }

    #[tokio::test]
    async fn fetch_metrics_mongod() {
        trace_init();
        test_instance("mongodb://localhost:27017").await;
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
        test_instance("mongodb://localhost:27019").await;
    }
}
