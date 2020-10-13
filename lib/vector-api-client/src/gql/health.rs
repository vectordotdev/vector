use async_trait::async_trait;
use graphql_client::GraphQLQuery;
use serde::{Deserialize, Serialize};

type DateTime = chrono::DateTime<chrono::Utc>;

#[derive(GraphQLQuery, Deserialize, Serialize)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/queries/health.graphql",
    response_derives = "Debug"
)]
pub struct HealthQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/heartbeat.graphql",
    response_derives = "Debug"
)]
pub struct HeartbeatSubscription;

#[async_trait]
pub trait HealthQueryExt {
    async fn health_query(&self) -> crate::QueryResult<HealthQuery>;
}

#[async_trait]
impl HealthQueryExt for crate::Client {
    async fn health_query(&self) -> crate::QueryResult<HealthQuery> {
        self.query::<HealthQuery>(&HealthQuery::build_query(health_query::Variables))
            .await
    }
}

#[async_trait]
pub trait HealthSubscriptionExt {
    async fn heartbeat_susbcription(
        &self,
        interval: i64,
    ) -> crate::SubscriptionResult<HeartbeatSubscription>;
}

#[async_trait]
impl HealthSubscriptionExt for crate::SubscriptionClient {
    async fn heartbeat_susbcription(
        &self,
        interval: i64,
    ) -> crate::SubscriptionResult<HeartbeatSubscription> {
        let request_body =
            HeartbeatSubscription::build_query(heartbeat_subscription::Variables { interval });

        self.start::<HeartbeatSubscription>(&request_body).await
    }
}
