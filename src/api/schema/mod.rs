pub mod components;
pub mod events;
pub mod filter;
mod health;
mod meta;
mod metrics;
mod relay;
pub mod sort;

use async_graphql::{EmptyMutation, MergedObject, MergedSubscription, Schema, SchemaBuilder};

#[derive(MergedObject, Default)]
pub struct Query(
    health::HealthQuery,
    components::ComponentsQuery,
    #[cfg(feature = "sources-host_metrics")] metrics::MetricsQuery,
    meta::MetaQuery,
);

#[derive(MergedSubscription, Default)]
pub struct Subscription(
    health::HealthSubscription,
    metrics::MetricsSubscription,
    components::ComponentsSubscription,
    events::EventsSubscription,
);

/// Build a new GraphQL schema, comprised of Query, Mutation and Subscription types
pub fn build_schema() -> SchemaBuilder<Query, EmptyMutation, Subscription> {
    Schema::build(Query::default(), EmptyMutation, Subscription::default())
}
