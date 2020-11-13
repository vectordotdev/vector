pub mod components;
mod health;
mod metrics;
mod vector;

use async_graphql::{EmptyMutation, MergedObject, MergedSubscription, Schema, SchemaBuilder};

#[derive(MergedObject, Default)]
pub struct Query(
    health::HealthQuery,
    components::ComponentsQuery,
    metrics::MetricsQuery,
    vector::VectorQuery,
);

#[derive(MergedSubscription, Default)]
pub struct Subscription(
    health::HealthSubscription,
    metrics::MetricsSubscription,
    components::ComponentsSubscription,
);

/// Build a new GraphQL schema, comprised of Query, Mutation and Subscription types
pub fn build_schema() -> SchemaBuilder<Query, EmptyMutation, Subscription> {
    Schema::build(Query::default(), EmptyMutation, Subscription::default())
}
