// TODO: In order to correctly assert component specification compliance, we would have to do some more advanced mocking
// off the endpoint, which would include also providing a mock OAuth2 endpoint to allow for generating a token from the
// mocked credentials. Let this TODO serve as a placeholder for doing that in the future.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use futures::{
    stream::{self, StreamExt},
    FutureExt,
};
use goauth::scopes::Scope;
use googleapis_proto::MetricServiceClient;
use tonic::{metadata::MetadataValue, service::Interceptor, transport::Channel, Code, Request};
use vector_common::internal_event::{ComponentEventsDropped, UNINTENTIONAL};
use vector_config::configurable_component;
use vector_core::event::MetricKind;
use vector_core::{event::EventArray, sink::StreamSink};

mod googleapis_proto {
    pub mod google {
        pub mod api {
            include!(concat!(env!("OUT_DIR"), "/google.api.rs"));
        }

        pub mod rpc {
            include!(concat!(env!("OUT_DIR"), "/google.rpc.rs"));
        }

        pub mod monitoring {
            pub mod v3 {
                include!(concat!(env!("OUT_DIR"), "/google.monitoring.v3.rs"));
            }
        }
    }

    pub use google::api::{
        metric_descriptor::{MetricKind, ValueType},
        Metric, MonitoredResource,
    };
    pub use google::monitoring::v3::{
        metric_service_client::MetricServiceClient, typed_value::Value, CreateTimeSeriesRequest,
        Point, TimeInterval, TimeSeries, TypedValue,
    };
}

use crate::{
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    event::{Metric, MetricValue},
    gcp::{GcpAuthConfig, GcpAuthenticator},
    sinks::{gcp, Healthcheck, VectorSink},
};

/// Configuration for the `gcp_stackdriver_metrics` sink.
#[configurable_component(sink("gcp_stackdriver_metrics"))]
#[derive(Clone, Debug, Default)]
pub struct StackdriverConfig {
    #[serde(skip, default = "default_endpoint")]
    endpoint: String,

    /// The project ID to which to publish metrics.
    ///
    /// See the [Google Cloud Platform project management documentation][project_docs] for more details.
    ///
    /// [project_docs]: https://cloud.google.com/resource-manager/docs/creating-managing-projects
    pub project_id: String,

    /// The monitored resource to associate the metrics with.
    pub resource: gcp::GcpTypedResource,

    #[serde(flatten)]
    pub auth: GcpAuthConfig,

    /// The default namespace to use for metrics that do not have one.
    ///
    /// Metrics with the same name can only be differentiated by their namespace, and not all
    /// metrics have their own namespace.
    #[serde(default = "default_metric_namespace_value")]
    pub default_namespace: String,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn default_metric_namespace_value() -> String {
    "namespace".to_string()
}

fn default_endpoint() -> String {
    "https://monitoring.googleapis.com".to_string()
}

impl_generate_config_from_default!(StackdriverConfig);

/// Maximum time series included in a write request.
/// https://cloud.google.com/monitoring/quotas#:~:text=Cloud%20Monitoring%20officially%20supports%20up,custom%20metrics%20or%20historical%20data.
const GCP_MAX_BATCH_SIZE: usize = 200;

/// We doubled the rate at which data can be written to a single time series
/// https://cloud.google.com/monitoring/quotas#:~:text=Cloud%20Monitoring%20officially%20supports%20up,custom%20metrics%20or%20historical%20data.
const GCP_SMALLEST_CREATE_TIMESERIES_CALL_FREQUENCY: Duration = Duration::from_secs(10);

const MAX_RETRIES_ON_LEGITIMATE_GCP_ERROR: usize = 3;

/// Metrics that this GCP sink currently supports.
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
enum MetricDataType {
    Counter,
    Gauge,
}

/// A simplified, intermediate metric representation that is used when building
/// GCP time series.
struct MetricDataPoint {
    name: String,
    namespace: String,
    r#type: MetricDataType,
    kind: MetricKind,
    value: f64,
    created: DateTime<Utc>,
    tags: Option<HashMap<String, String>>,
}

impl MetricDataPoint {
    fn from_metric(
        config: &StackdriverConfig,
        start_time: DateTime<Utc>,
        mut metric: Metric,
    ) -> Option<Self> {
        let name = metric.name().to_string();
        let namespace = metric
            .take_namespace()
            .unwrap_or_else(|| config.default_namespace.clone());

        let (r#type, value) = match metric.value() {
            MetricValue::Counter { value } => (MetricDataType::Counter, *value),
            MetricValue::Gauge { value } => (MetricDataType::Gauge, *value),
            _ => return None,
        };

        let created = if let Some(time) = metric.data().timestamp() {
            *time
        } else {
            Utc::now()
        };

        if created < start_time {
            return None;
        }

        let tags = if let Some(tags) = metric.tags() {
            let tags = tags
                .iter_single()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect::<HashMap<String, String>>();

            Some(tags)
        } else {
            None
        };

        Some(Self {
            name,
            namespace,
            r#type,
            value,
            created,
            tags,
            kind: metric.kind(),
        })
    }

    fn merge(&mut self, other: &Self) {
        match self.r#type {
            MetricDataType::Counter => match self.kind {
                MetricKind::Incremental => self.value += other.value,
                MetricKind::Absolute => self.value = other.value,
            },

            MetricDataType::Gauge => {
                self.value = other.value;
            }
        }
    }
}

/// Creates an interceptor that will attach a GCP token to every request.
fn interceptor(auth: GcpAuthenticator) -> impl Interceptor {
    let token = auth.make_token();

    move |mut req: Request<()>| {
        if let Some(token) = token.as_ref() {
            let meta = MetadataValue::try_from(token).expect("a valid metadata value");
            req.metadata_mut().insert("authorization", meta);
        }

        Ok(req)
    }
}

/// Turns a UTC time into a grpc timestamp.
fn timestamp_proto(time: DateTime<Utc>) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: time.timestamp(),
        nanos: time.timestamp_nanos() as i32,
    }
}

/// Creates a GCP time series out of a metric data point.
fn gcp_timeseries(
    config: &StackdriverConfig,
    start_time: DateTime<Utc>,
    data_point: MetricDataPoint,
) -> googleapis_proto::TimeSeries {
    let end_time = data_point.created;
    let start_time = if data_point.r#type == MetricDataType::Gauge {
        end_time
    } else {
        start_time
    };

    let metric_kind = match data_point.r#type {
        MetricDataType::Gauge => googleapis_proto::MetricKind::Gauge,
        MetricDataType::Counter => match data_point.kind {
            MetricKind::Absolute => googleapis_proto::MetricKind::Gauge,
            MetricKind::Incremental => googleapis_proto::MetricKind::Cumulative,
        },
    };

    let value_type = match data_point.r#type {
        MetricDataType::Gauge => googleapis_proto::ValueType::Double,
        MetricDataType::Counter => googleapis_proto::ValueType::Int64,
    };

    let value = match data_point.r#type {
        MetricDataType::Gauge => googleapis_proto::Value::DoubleValue(data_point.value),
        MetricDataType::Counter => googleapis_proto::Value::Int64Value(data_point.value as i64),
    };

    let unit = value_type.as_str_name().to_string();

    googleapis_proto::TimeSeries {
        metric: Some(googleapis_proto::Metric {
            r#type: format!(
                "custom.googleapis.com/{}/metrics/{}",
                data_point.namespace, data_point.name
            ),
            labels: data_point.tags.unwrap_or_else(Default::default),
        }),
        resource: Some(googleapis_proto::MonitoredResource {
            r#type: config.resource.r#type.clone(),
            labels: config.resource.labels.clone(),
        }),
        metadata: None,
        metric_kind: metric_kind.into(),
        value_type: value_type.into(),
        points: vec![googleapis_proto::Point {
            interval: Some(googleapis_proto::TimeInterval {
                end_time: Some(timestamp_proto(end_time)),
                start_time: Some(timestamp_proto(start_time)),
            }),

            value: Some(googleapis_proto::TypedValue { value: Some(value) }),
        }],
        unit,
    }
}

struct GrpcEventSink {
    config: StackdriverConfig,
}

#[async_trait::async_trait]
impl StreamSink<EventArray> for GrpcEventSink {
    async fn run(self: Box<Self>, mut input: stream::BoxStream<'_, EventArray>) -> Result<(), ()> {
        let auth = match self.config.auth.build(Scope::MonitoringWrite).await {
            Ok(auth) => auth,
            Err(e) => {
                error!(message = "Fatal gcp_stackdriver_metrics error.", %e);
                return Err(());
            }
        };

        let uri = if let Ok(uri) = self.config.endpoint.as_str().parse::<http::Uri>() {
            uri
        } else {
            error!(message = "Error when parsing uri.", %self.config.endpoint);
            return Err(());
        };

        let channel = Channel::builder(uri)
            .tls_config(tonic::transport::ClientTlsConfig::new())
            .map_err(|e| {
                error!(message = "Fatal gcp_stackdriver_metrics error.", %e);
            })?
            .connect_lazy();

        let mut stopwatch = tokio::time::interval(Duration::from_secs(1));
        let mut aggregate = HashMap::<String, MetricDataPoint>::with_capacity(GCP_MAX_BATCH_SIZE);
        let mut time_tracker = StartTimeTracker::new();
        let mut last_time = Instant::now();
        let mut buffer = Vec::<MetricDataPoint>::new();
        let mut client =
            MetricServiceClient::<Channel>::with_interceptor(channel, interceptor(auth));

        loop {
            if aggregate.len() == GCP_MAX_BATCH_SIZE {
                // In case we already accumulated the maximum unique data point count supported by
                // GCP, we wait so we are not denied by the GCP backend for sending too many
                // requests.
                while last_time.elapsed() < GCP_SMALLEST_CREATE_TIMESERIES_CALL_FREQUENCY {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            } else {
                while let Some(data_point) = buffer.pop() {
                    aggregate
                        .entry(format!("{}/{}", data_point.namespace, data_point.name))
                        .and_modify(|cur| cur.merge(&data_point))
                        .or_insert(data_point);

                    if aggregate.len() == GCP_MAX_BATCH_SIZE {
                        break;
                    }
                }
            }

            // We send a timeseries create request as long as we have at least one metric data point and
            // we waited enough since the last time we sent something to GCP.
            if !aggregate.is_empty()
                && last_time.elapsed() >= GCP_SMALLEST_CREATE_TIMESERIES_CALL_FREQUENCY
            {
                let start_time = time_tracker.start_time();
                let time_series = aggregate
                    .drain()
                    .map(|t| gcp_timeseries(&self.config, start_time, t.1))
                    .collect::<Vec<_>>();

                let req = googleapis_proto::CreateTimeSeriesRequest {
                    name: format!("projects/{}", self.config.project_id),
                    time_series,
                };

                let mut attempts = 1usize;

                loop {
                    if let Err(status) = client.create_time_series(Request::new(req.clone())).await
                    {
                        if status.code() == Code::Internal
                            || status.code() == Code::Unknown
                                && attempts < MAX_RETRIES_ON_LEGITIMATE_GCP_ERROR
                        {
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            attempts += 1;
                            continue;
                        }

                        error!(message = "Error when sending time_series", %status);
                        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                            count: attempts,
                            reason: "Error when sending timeseries to GCP",
                        });
                    }

                    break;
                }

                last_time = Instant::now();
            }

            tokio::select! {
                _ = stopwatch.tick() => {}

                events = input.next() => {
                    if events.is_none() {
                        return Ok(());
                    }

                    if let EventArray::Metrics(metrics) = events.unwrap() {
                        let start_time = time_tracker.start_time();
                        buffer.extend(metrics.into_iter().filter_map(|m| MetricDataPoint::from_metric(&self.config, start_time, m)));
                    }
                }
            }
        }
    }
}

/// The end time must not be earlier than the start time, and the end time must not be more than 25 hours
/// in the past or more than five minutes in the future.
/// https://cloud.google.com/monitoring/api/ref_v3/rpc/google.monitoring.v3#timeinterval
const GCP_25_HOURS_TIME_WINDOW: Duration = Duration::from_secs(25 * 3_600);

/// Keeps track of the start time that would be use when sending time series to GCP.
struct StartTimeTracker {
    time: DateTime<Utc>,
    clock: Instant,
}

impl StartTimeTracker {
    fn new() -> Self {
        Self {
            time: Utc::now(),
            clock: Instant::now(),
        }
    }

    /// Returns the current window starting time.
    fn start_time(&mut self) -> DateTime<Utc> {
        if self.clock.elapsed() >= GCP_25_HOURS_TIME_WINDOW {
            self.time = Utc::now();
            self.clock = Instant::now();
        }

        self.time
    }
}

#[async_trait::async_trait]
impl SinkConfig for StackdriverConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let healthcheck = healthcheck().boxed();
        let sink = GrpcEventSink {
            config: self.clone(),
        };

        Ok((VectorSink::Stream(Box::new(sink)), healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

async fn healthcheck() -> crate::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::StackdriverConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StackdriverConfig>();
    }
}
