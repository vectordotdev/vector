mod health;
mod metrics;
pub mod topology;

use async_graphql::{EmptyMutation, GQLMergedObject, GQLMergedSubscription, Schema, SchemaBuilder};

#[derive(GQLMergedObject, Default)]
pub struct Query(health::HealthQuery, topology::TopologyQuery);

#[derive(GQLMergedSubscription, Default)]
pub struct Subscription(health::HealthSubscription, metrics::MetricsSubscription);

/// Build a new GraphQL schema, comprised of Query, Mutation and Subscription types
pub fn build_schema() -> SchemaBuilder<Query, EmptyMutation, Subscription> {
    Schema::build(Query::default(), EmptyMutation, Subscription::default())
}
