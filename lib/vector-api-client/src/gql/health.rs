//! Health queries/subscriptions, for asserting a GraphQL API server is alive.

use graphql_client::GraphQLQuery;

/// Shorthand for a Chrono datetime, set to UTC.
type DateTime = chrono::DateTime<chrono::Utc>;

/// HealthQuery is generally used to assert that the GraphQL API server is alive.
/// The `health` field returns true.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/queries/health.graphql",
    response_derives = "Debug"
)]
pub struct HealthQuery;

/// HeartbeatSubscription is a subscription that returns a 'heartbeat' in the form
/// of a UTC timestamp. The use-case is allowing a client to assert that the server is
/// sending regular payloads, by using the timestamp to determine when the last healthcheck
/// was successful.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/heartbeat.graphql",
    response_derives = "Debug"
)]
pub struct HeartbeatSubscription;

/// Extension methods for health queries.
pub trait HealthQueryExt {
    /// Executes a health query.
    async fn health_query(&self) -> crate::QueryResult<HealthQuery>;
}

impl HealthQueryExt for crate::Client {
    /// Executes a health query.
    async fn health_query(&self) -> crate::QueryResult<HealthQuery> {
        self.query::<HealthQuery>(&HealthQuery::build_query(health_query::Variables))
            .await
    }
}

/// Extension methods for health subscriptions
pub trait HealthSubscriptionExt {
    /// Executes a heartbeat subscription, on a millisecond `interval`.
    fn heartbeat_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<HeartbeatSubscription>;
}

impl HealthSubscriptionExt for crate::SubscriptionClient {
    /// Executes a heartbeat subscription, on a millisecond `interval`.
    fn heartbeat_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<HeartbeatSubscription> {
        let request_body =
            HeartbeatSubscription::build_query(heartbeat_subscription::Variables { interval });

        self.start::<HeartbeatSubscription>(&request_body)
    }
}
