use crate::{
    config::{self, GlobalOptions, SourceConfig, SourceDescription},
    event::metric::{Metric, MetricKind, MetricValue},
    internal_events::{MongoDBMetricsBsonParseError, MongoDBMetricsRequestError},
    shutdown::ShutdownSignal,
    Event, Pipeline,
};
use chrono::Utc;
use futures::{
    compat::{Future01CompatExt, Sink01CompatExt},
    future, stream, FutureExt, StreamExt, TryFutureExt,
};
use futures01::Sink;
use mongodb::{
    bson::{self, doc, from_document},
    error::Error as MongoError,
    options::ClientOptions,
    Client,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{collections::BTreeMap, sync::Arc};
use tokio::time::{interval, Duration};
use url::{ParseError, Url};

mod types;
use types::{CommandIsMaster, CommandServerStatus, NodeType};

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
        MetricValue::Counter { value: $value }
    };
}

macro_rules! gauge {
    ($value:expr) => {
        MetricValue::Gauge { value: $value }
    };
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("invalid endpoint: {:?}", source))]
    InvalidEndpoint { source: MongoError },
    #[snafu(display("invalid client options: {:?}", source))]
    InvalidClientOptions { source: MongoError },
    #[snafu(display("invalid endpoint: {:?}", source))]
    ParseEndpointError { source: ParseError },
    #[snafu(display("error on username/password removal: {:?}", error))]
    SanitizeEndpointError { error: &'static str },
    #[snafu(display("failed to execute `isMaster` command: {:?}", source))]
    CommandIsMasterMongoError { source: MongoError },
    #[snafu(display("failed to parse `isMaster` response: {:?}", source))]
    CommandIsMasterParseError { source: bson::de::Error },
    #[snafu(display("only `Mongod` supported right now, current: `{:?}`", node_type))]
    UnsupportedNodeType { node_type: NodeType },
}

#[derive(Debug)]
enum CollectError {
    Mongo(MongoError),
    Bson(bson::de::Error),
}

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
    endpoint: String,
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
    /// Works only with Standalone connection-string. Collect metrics only from specified instance.
    /// https://docs.mongodb.com/manual/reference/connection-string/#standard-connection-string-format
    async fn new(endpoint: &str, namespace: &str) -> Result<Arc<MongoDBMetrics>, BuildError> {
        let mut tags: BTreeMap<String, String> = BTreeMap::new();

        let mut client_options = ClientOptions::parse(endpoint)
            .await
            .context(InvalidEndpoint)?;
        client_options.direct_connection = Some(true);
        tags.insert("host".into(), client_options.hosts[0].hostname.clone());
        let client = Client::with_options(client_options).context(InvalidClientOptions)?;

        let endpoint = Self::sanitize_endpoint(endpoint)?;
        tags.insert("endpoint".into(), endpoint.clone());

        // TODO: debug! with node version

        let node_type = Self::get_node_type(&client).await?;
        if node_type != NodeType::Mongod {
            return Err(BuildError::UnsupportedNodeType { node_type });
        }

        Ok(Arc::new(Self {
            client,
            endpoint,
            namespace: namespace.to_owned(),
            tags,
        }))
    }

    /// Remove `username` and `password` from URL endpoint.
    fn sanitize_endpoint(endpoint: &str) -> Result<String, BuildError> {
        let mut url = Url::parse(endpoint).context(ParseEndpointError)?;
        url.set_username("")
            .map_err(|_| BuildError::SanitizeEndpointError {
                error: "failed to remove username from MongoDB endpoint",
            })?;
        url.set_password(None)
            .map_err(|_| BuildError::SanitizeEndpointError {
                error: "failed to remove password from MongoDB endpoint",
            })?;
        Ok(url.to_string())
    }

    /// Finding node type for client with `isMaster` command.
    async fn get_node_type(client: &Client) -> Result<NodeType, BuildError> {
        let doc = client
            .database("admin")
            .run_command(doc! { "isMaster": 1 }, None)
            .await
            .context(CommandIsMasterMongoError)?;
        let msg: CommandIsMaster = from_document(doc).context(CommandIsMasterParseError)?;

        Ok(if msg.set_name.is_some() && msg.hosts.is_some() {
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

    fn encode_namespace(&self, name: &str) -> String {
        match self.namespace.as_str() {
            "" => name.to_string(),
            _ => format!("{}_{}", self.namespace, name),
        }
    }

    fn create_metric(
        &self,
        name: &str,
        value: MetricValue,
        tags: BTreeMap<String, String>,
    ) -> Metric {
        Metric {
            name: self.encode_namespace(name),
            timestamp: Some(Utc::now()),
            tags: Some(tags),
            kind: MetricKind::Absolute,
            value: value,
        }
    }

    async fn collect(self: Arc<Self>) -> stream::BoxStream<'static, Metric> {
        let (up_value, metrics) = match Arc::clone(&self).collect_server_status().await {
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

        stream::once(future::ready(self.create_metric(
            "up",
            gauge!(up_value),
            tags!(self.tags),
        )))
        .chain(stream::iter(metrics))
        .boxed()
    }

    async fn collect_server_status(self: Arc<Self>) -> Result<Vec<Metric>, CollectError> {
        let mut metrics = vec![];

        let doc = self
            .client
            .database("admin")
            .run_command(doc! { "serverStatus": 1 }, None)
            .await
            .map_err(CollectError::Mongo)?;
        let status: CommandServerStatus = from_document(doc).map_err(CollectError::Bson)?;

        // asserts_total
        metrics.push(self.create_metric(
            "asserts_total",
            counter!(status.asserts.regular as f64),
            tags!(self.tags, "type" => "regular"),
        ));
        metrics.push(self.create_metric(
            "asserts_total",
            counter!(status.asserts.warning as f64),
            tags!(self.tags, "type" => "warning"),
        ));
        metrics.push(self.create_metric(
            "asserts_total",
            counter!(status.asserts.msg as f64),
            tags!(self.tags, "type" => "msg"),
        ));
        metrics.push(self.create_metric(
            "asserts_total",
            counter!(status.asserts.user as f64),
            tags!(self.tags, "type" => "user"),
        ));
        metrics.push(self.create_metric(
            "asserts_total",
            counter!(status.asserts.rollovers as f64),
            tags!(self.tags, "type" => "rollovers"),
        ));

        // connections
        metrics.push(self.create_metric(
            "connections",
            counter!(status.connections.active as f64),
            tags!(self.tags, "state" => "active"),
        ));
        metrics.push(self.create_metric(
            "connections",
            counter!(status.connections.available as f64),
            tags!(self.tags, "state" => "available"),
        ));
        metrics.push(self.create_metric(
            "connections",
            counter!(status.connections.current as f64),
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
            gauge!(status.instance.local_time.timestamp() as f64),
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
            gauge!(status.memory.resident as f64),
            tags!(self.tags, "type" => "resident"),
        ));
        metrics.push(self.create_metric(
            "memory",
            gauge!(status.memory.r#virtual as f64),
            tags!(self.tags, "type" => "virtual"),
        ));
        if let Some(value) = status.memory.mapped {
            metrics.push(self.create_metric(
                "memory",
                gauge!(value as f64),
                tags!(self.tags, "type" => "mapped"),
            ))
        }
        if let Some(value) = status.memory.mapped_with_journal {
            metrics.push(self.create_metric(
                "memory",
                gauge!(value as f64),
                tags!(self.tags, "type" => "mapped_with_journal"),
            ))
        }

        Ok(metrics)
    }
}
