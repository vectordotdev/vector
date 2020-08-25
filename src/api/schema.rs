use crate::event::Metric;
use crate::metrics::capture_metrics;
use crate::{metrics, Event};
use async_graphql::validators::IntRange;
use async_graphql::{
    EmptyMutation, FieldResult, Object, Schema, SchemaBuilder, SimpleObject, Subscription,
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

#[Object]
impl Metric {
    /// Metric name
    async fn name(&self) -> String {
        self.name.clone()
    }

    /// Metric timestamp
    async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.timestamp
    }
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
    ) -> impl Stream<Item = Metric> {
        get_metrics(interval)
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
            for ev in capture_metrics(&controller) {
                if let Event::Metric(m) = ev {
                    yield m
                }
            }
        }
    }
}
