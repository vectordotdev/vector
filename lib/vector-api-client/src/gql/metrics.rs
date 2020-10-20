//! Metrics queries/subscriptions

use crate::SubscriptionResult;
use async_trait::async_trait;
use graphql_client::GraphQLQuery;

/// UptimeMetricsSubscription returns uptime metrics to determine how long the Vector
/// instance has been running
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/uptime_metrics.graphql",
    response_derives = "Debug"
)]
pub struct UptimeMetricsSubscription;

/// EventsProcessedTotalMetricsSubscription contains metrics on the number of events
/// that have been processed by a Vector instance
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/events_processed_total_metrics.graphql",
    response_derives = "Debug"
)]
pub struct EventsProcessedTotalMetricsSubscription;

/// Extension methods for metrics subscriptions
#[async_trait]
pub trait MetricsSubscriptionExt {
    /// Executes an uptime metrics subscription
    async fn uptime_metrics_subscription(
        &self,
    ) -> crate::SubscriptionResult<UptimeMetricsSubscription>;

    /// Executes an events processed metrics subscription
    async fn events_processed_total_metrics_subscription(
        &self,
        interval: i64,
    ) -> crate::SubscriptionResult<EventsProcessedTotalMetricsSubscription>;
}

#[async_trait]
impl MetricsSubscriptionExt for crate::SubscriptionClient {
    /// Executes an uptime metrics subscription
    async fn uptime_metrics_subscription(&self) -> SubscriptionResult<UptimeMetricsSubscription> {
        let request_body =
            UptimeMetricsSubscription::build_query(uptime_metrics_subscription::Variables);

        self.start::<UptimeMetricsSubscription>(&request_body).await
    }

    /// Executes an events processed metrics subscription
    async fn events_processed_total_metrics_subscription(
        &self,
        interval: i64,
    ) -> SubscriptionResult<EventsProcessedTotalMetricsSubscription> {
        let request_body = EventsProcessedTotalMetricsSubscription::build_query(
            events_processed_total_metrics_subscription::Variables { interval },
        );

        self.start::<EventsProcessedTotalMetricsSubscription>(&request_body)
            .await
    }
}
