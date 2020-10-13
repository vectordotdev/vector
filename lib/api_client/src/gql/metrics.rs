use crate::SubscriptionResult;
use async_trait::async_trait;
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

#[async_trait]
pub trait MetricsSubscriptionExt {
    async fn uptime_metrics_subscription(
        &self,
    ) -> crate::SubscriptionResult<UptimeMetricsSubscription>;

    async fn events_processed_metrics_subscription(
        &self,
        interval: i64,
    ) -> crate::SubscriptionResult<EventsProcessedMetricsSubscription>;
}

#[async_trait]
impl MetricsSubscriptionExt for crate::SubscriptionClient {
    async fn uptime_metrics_subscription(&self) -> SubscriptionResult<UptimeMetricsSubscription> {
        let request_body =
            UptimeMetricsSubscription::build_query(uptime_metrics_subscription::Variables);

        self.start::<UptimeMetricsSubscription>(&request_body).await
    }

    async fn events_processed_metrics_subscription(
        &self,
        interval: i64,
    ) -> SubscriptionResult<EventsProcessedMetricsSubscription> {
        let request_body = EventsProcessedMetricsSubscription::build_query(
            events_processed_metrics_subscription::Variables { interval },
        );

        self.start::<EventsProcessedMetricsSubscription>(&request_body)
            .await
    }
}
