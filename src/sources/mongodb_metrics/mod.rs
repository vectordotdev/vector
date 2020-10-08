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
    bson::{doc, document::ValueAccessError},
    error::Error as MongoError,
    options::ClientOptions,
    Client,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
use tokio::time::{interval, Duration};
use url::Url;

#[derive(Debug)]
enum CollectError {
    Mongo(MongoError),
    Bson(ValueAccessError),
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
    async fn new(endpoint: &str, namespace: &str) -> crate::Result<Arc<MongoDBMetrics>> {
        let client_options = ClientOptions::parse(endpoint).await?;
        let endpoint = sanitize_endpoint(endpoint)?;

        let mut tags: BTreeMap<String, String> = BTreeMap::new();
        // TODO: Works only in Standalone mode
        tags.insert("host".into(), client_options.hosts[0].hostname.clone());
        tags.insert("endpoint".into(), endpoint.clone());

        Ok(Arc::new(Self {
            client: Client::with_options(client_options)?,
            endpoint,
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

    async fn collect(self: Arc<Self>) -> stream::BoxStream<'static, Metric> {
        match Arc::clone(&self).collect_server_status().await {
            Ok(metrics) => stream::iter(metrics).boxed(),
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

                stream::once(future::ready(Metric {
                    name: self.encode_namespace("up"),
                    timestamp: Some(Utc::now()),
                    tags: Some(self.tags.clone()),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.0 },
                }))
                .boxed()
            }
        }
    }

    async fn collect_server_status(self: Arc<Self>) -> Result<Vec<Metric>, CollectError> {
        let mut metrics = vec![];

        let db = self.client.database("admin");
        let status = db
            .run_command(doc! { "serverStatus": 1 }, None)
            .await
            .map_err(CollectError::Mongo)?;

        // asserts_total
        let doc = status.get_document("asserts").map_err(CollectError::Bson)?;
        macro_rules! add_assert_metric {
            ($name:expr) => {
                metrics.push(Metric {
                    name: self.encode_namespace("asserts_total"),
                    timestamp: Some(Utc::now()),
                    tags: {
                        let mut tags = self.tags.clone();
                        tags.insert("type".into(), $name.into());
                        Some(tags)
                    },
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter {
                        value: doc.get_i32($name).map_err(CollectError::Bson)? as f64,
                    },
                });
            };
        }
        add_assert_metric!("regular");
        add_assert_metric!("warning");
        add_assert_metric!("msg");
        add_assert_metric!("user");
        add_assert_metric!("rollovers");

        // connections
        let doc = status
            .get_document("connections")
            .map_err(CollectError::Bson)?;
        macro_rules! add_connection_metric {
            ($name:expr) => {
                metrics.push(Metric {
                    name: self.encode_namespace("connections"),
                    timestamp: Some(Utc::now()),
                    tags: {
                        let mut tags = self.tags.clone();
                        tags.insert("state".into(), $name.into());
                        Some(tags)
                    },
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge {
                        value: doc.get_i32($name).map_err(CollectError::Bson)? as f64,
                    },
                })
            };
        }
        add_connection_metric!("active");
        add_connection_metric!("available");
        add_connection_metric!("current");

        // extra_info_*
        let doc = status
            .get_document("extra_info")
            .map_err(CollectError::Bson)?;
        if doc.contains_key("heap_usage_bytes") {
            metrics.push(Metric {
                name: self.encode_namespace("extra_info_heap_usage_bytes"),
                timestamp: Some(Utc::now()),
                tags: Some(self.tags.clone()),
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: doc
                        .get_i64("heap_usage_bytes")
                        .map_err(CollectError::Bson)? as f64,
                },
            });
        }
        metrics.push(Metric {
            name: self.encode_namespace("extra_info_page_faults"),
            timestamp: Some(Utc::now()),
            tags: Some(self.tags.clone()),
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge {
                value: doc.get_i64("page_faults").map_err(CollectError::Bson)? as f64,
            },
        });

        // instance_*
        metrics.push(Metric {
            name: self.encode_namespace("instance_local_time"),
            timestamp: Some(Utc::now()),
            tags: Some(self.tags.clone()),
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge {
                value: doc
                    .get_datetime("localTime")
                    .map_err(CollectError::Bson)?
                    .timestamp() as f64,
            },
        });
        metrics.push(Metric {
            name: self.encode_namespace("instance_uptime_estimate_seconds_total"),
            timestamp: Some(Utc::now()),
            tags: Some(self.tags.clone()),
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge {
                value: doc.get_i64("uptimeEstimate").map_err(CollectError::Bson)? as f64,
            },
        });
        metrics.push(Metric {
            name: self.encode_namespace("instance_uptime_seconds_total"),
            timestamp: Some(Utc::now()),
            tags: Some(self.tags.clone()),
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge {
                value: doc.get_f64("uptime").map_err(CollectError::Bson)?,
            },
        });

        // memory
        let doc = status.get_document("mem").map_err(CollectError::Bson)?;
        macro_rules! add_memroy_metric {
            ($name:expr, $type:expr) => {
                metrics.push(Metric {
                    name: self.encode_namespace("memory"),
                    timestamp: Some(Utc::now()),
                    tags: {
                        let mut tags = self.tags.clone();
                        tags.insert("type".into(), $type.into());
                        Some(tags)
                    },
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge {
                        value: doc.get_i32($name).map_err(CollectError::Bson)? as f64,
                    },
                })
            };
        }
        add_memroy_metric!("resident", "resident");
        add_memroy_metric!("virtual", "virtual");
        add_memroy_metric!("mapped", "mapped");
        add_memroy_metric!("mappedWithJournal", "mapped_with_journal");

        Ok(metrics)
    }
}

// TODO: need to be improved (unwrap, standalone mode)
fn sanitize_endpoint(endpoint: &str) -> crate::Result<String> {
    let mut url = Url::parse(endpoint)?;
    url.set_username("").unwrap();
    url.set_password(None).unwrap();
    Ok(url.to_string())
}
