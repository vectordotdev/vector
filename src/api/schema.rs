use async_graphql::validators::IntRange;
use async_graphql::{
    EmptyMutation, FieldResult, Object, Schema, SchemaBuilder, SimpleObject, Subscription,
};
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
    async fn heartbeat(
        &self,
        #[arg(default = 1000, validator(IntRange(min = "100", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Heartbeat> {
        tokio::time::interval(Duration::from_millis(interval as u64)).map(|_| Heartbeat::new())
    }
}

/// Build a new GraphQL schema, comprised of Query, Mutation and Subscription types
pub fn build_schema() -> SchemaBuilder<Query, EmptyMutation, Subscription> {
    Schema::build(Query, EmptyMutation, Subscription)
}
