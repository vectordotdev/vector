use crate::event::{Event, Metric, MetricValue};
use crate::metrics;
use async_graphql::validators::IntRange;
use async_graphql::{
    EmptyMutation, FieldResult, Interface, Object, Schema, SchemaBuilder, SimpleObject,
    Subscription,
};
use async_stream::stream;
use chrono::{DateTime, Utc};
use tokio::stream::{Stream, StreamExt};
use tokio::time::Duration;

#[SimpleObject]
struct Heartbeat {
    utc: DateTime<Utc>,
}

impl Heartbeat {
    fn new() -> Self {
        Heartbeat { utc: Utc::now() }
    }
}

struct EventsProcessed(Metric);

#[Object]
impl EventsProcessed {
    /// Metric timestamp
    async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp
    }

    /// Number of events processed
    async fn events_processed(&self) -> f64 {
        match self.0.value {
            MetricValue::Counter { value } => value,
            _ => 0.00,
        }
    }
}

impl Into<EventsProcessed> for Metric {
    fn into(self) -> EventsProcessed {
        EventsProcessed(self)
    }
}

struct BytesProcessed(Metric);

#[Object]
impl BytesProcessed {
    /// Metric timestamp
    async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp
    }

    /// Number of bytes processed
    async fn bytes_processed(&self) -> f64 {
        match self.0.value {
            MetricValue::Counter { value } => value,
            _ => 0.00,
        }
    }
}

#[Interface(field(name = "timestamp", type = "Option<DateTime<Utc>>"))]
enum MetricType {
    EventsProcessed(EventsProcessed),
    BytesProcessed(BytesProcessed),
}

pub struct Query;

#[Object]
impl Query {
    /// Returns `true` to denote the GraphQL server is reachable
    async fn health(&self) -> FieldResult<bool> {
        Ok(true)
    }
}

pub struct Subscription;

#[Subscription]
impl Subscription {
    /// Heartbeat, containing the UTC timestamp of the last server-sent payload
    async fn heartbeat(
        &self,
        #[arg(default = 1000, validator(IntRange(min = "100", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Heartbeat> {
        tokio::time::interval(Duration::from_millis(interval as u64)).map(|_| Heartbeat::new())
    }

    /// Returns all Vector metrics, aggregated at the provided millisecond interval
    async fn metrics(
        &self,
        #[arg(validator(IntRange(min = "100", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = MetricType> {
        get_metrics(interval).filter_map(|m| match m.name.as_str() {
            "events_processed" => Some(MetricType::EventsProcessed(m.into())),
            _ => None,
        })
    }
}

/// Build a new GraphQL schema, comprised of Query, Mutation and Subscription types
pub fn build_schema() -> SchemaBuilder<Query, EmptyMutation, Subscription> {
    Schema::build(Query, EmptyMutation, Subscription)
}

/// Returns a stream of `Metric`s, collected at the provided millisecond interval
fn get_metrics(interval: i32) -> impl Stream<Item = Metric> {
    let controller = metrics::get_controller().unwrap();
    let mut interval = tokio::time::interval(Duration::from_millis(interval as u64));

    stream! {
        while let _ = interval.tick().await {
            for ev in metrics::capture_metrics(&controller) {
                if let Event::Metric(m) = ev {
                    yield m
                }
            }
        }
    }
}
