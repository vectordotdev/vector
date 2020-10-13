use graphql_client::GraphQLQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/uptime_metrics.graphql",
    response_derives = "Debug"
)]
pub struct UptimeMetricsSubscription;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/events_processed_metrics.graphql",
    response_derives = "Debug"
)]
pub struct EventsProcessedMetricsSubscription;
