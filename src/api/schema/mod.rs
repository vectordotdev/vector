mod health;
mod metrics;

use async_graphql::{
    validators::IntRange, EmptyMutation, GQLMergedObject, Schema, SchemaBuilder, Subscription,
};
use tokio::stream::Stream;

#[derive(GQLMergedObject, Default)]
pub struct Query(health::HealthQuery);

pub struct Subscription;

// Subscriptions in async-graphql can't currently be merged. Workaround for now by defining
// in one place.
// TODO(Lee) - break this out after https://github.com/async-graphql/async-graphql/issues/231#issuecomment-680863034
#[Subscription]
impl Subscription {
    /// Heartbeat, containing the UTC timestamp of the last server-sent payload
    async fn heartbeat(
        &self,
        #[arg(default = 1000, validator(IntRange(min = "100", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = health::Heartbeat> {
        health::heartbeat_stream(interval)
    }

    /// Returns all Vector metrics, aggregated at the provided millisecond interval
    async fn metrics(
        &self,
        #[arg(validator(IntRange(min = "100", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = metrics::MetricType> {
        metrics::metrics_stream(interval)
    }
}

/// Build a new GraphQL schema, comprised of Query, Mutation and Subscription types
pub fn build_schema() -> SchemaBuilder<Query, EmptyMutation, Subscription> {
    Schema::build(Query::default(), EmptyMutation, Subscription)
}
