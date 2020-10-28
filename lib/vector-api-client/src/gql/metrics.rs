//! Metrics queries/subscriptions

use crate::SubscriptionResult;
use async_trait::async_trait;
use graphql_client::GraphQLQuery;

/// UptimeSubscription returns uptime metrics to determine how long the Vector
/// instance has been running
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/uptime.graphql",
    response_derives = "Debug"
)]
pub struct UptimeSubscription;

/// EventsProcessedTotalSubscription contains metrics on the number of events
/// that have been processed by a Vector instance
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/events_processed_total.graphql",
    response_derives = "Debug"
)]
pub struct EventsProcessedTotalSubscription;

/// ComponentEventsProcessedTotalSubscription contains metrics on the number of events
/// that have been processed by a Vector instance, against a specific component
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_events_processed_total.graphql",
    response_derives = "Debug"
)]
pub struct ComponentEventsProcessedTotalSubscription;

/// Extension methods for metrics subscriptions
#[async_trait]
pub trait MetricsSubscriptionExt {
    /// Executes an uptime metrics subscription
    async fn uptime_subscription(&self) -> crate::SubscriptionResult<UptimeSubscription>;

    /// Executes an events processed metrics subscription
    async fn events_processed_total_subscription(
        &self,
        interval: i64,
    ) -> crate::SubscriptionResult<EventsProcessedTotalSubscription>;

    /// Executes a components events processed total metrics subscription
    async fn component_events_processed_total_subscription(
        &self,
        interval: i64,
    ) -> crate::SubscriptionResult<ComponentEventsProcessedTotalSubscription>;
}

#[async_trait]
impl MetricsSubscriptionExt for crate::SubscriptionClient {
    /// Executes an uptime metrics subscription
    async fn uptime_subscription(&self) -> SubscriptionResult<UptimeSubscription> {
        let request_body = UptimeSubscription::build_query(uptime_subscription::Variables);

        self.start::<UptimeSubscription>(&request_body).await
    }

    /// Executes an events processed metrics subscription
    async fn events_processed_total_subscription(
        &self,
        interval: i64,
    ) -> SubscriptionResult<EventsProcessedTotalSubscription> {
        let request_body = EventsProcessedTotalSubscription::build_query(
            events_processed_total_subscription::Variables { interval },
        );

        self.start::<EventsProcessedTotalSubscription>(&request_body)
            .await
    }

    /// Executes a components events processed total metrics subscription
    async fn component_events_processed_total_subscription(
        &self,
        interval: i64,
    ) -> SubscriptionResult<ComponentEventsProcessedTotalSubscription> {
        let request_body = ComponentEventsProcessedTotalSubscription::build_query(
            component_events_processed_total_subscription::Variables { interval },
        );

        self.start::<ComponentEventsProcessedTotalSubscription>(&request_body)
            .await
    }
}
