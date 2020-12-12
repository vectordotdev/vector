use crate::{BoxedSubscription, QueryResult};
use async_trait::async_trait;
use graphql_client::GraphQLQuery;

/// Component links query for returning linked components for sources, transforms, and sinks
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "tests/queries/component_links.graphql",
    response_derives = "Debug"
)]
pub struct ComponentLinksQuery;

/// Errors total subscription
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "tests/subscriptions/errors_total.graphql",
    response_derives = "Debug"
)]
pub struct ErrorsTotalSubscription;

/// Component errors totals subscription
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "tests/subscriptions/component_errors_totals.graphql",
    response_derives = "Debug"
)]
pub struct ComponentErrorsTotalsSubscription;

#[async_trait]
pub trait TestQueryExt {
    async fn component_links_query(&self) -> crate::QueryResult<ComponentLinksQuery>;
}

#[async_trait]
impl TestQueryExt for crate::Client {
    async fn component_links_query(&self) -> QueryResult<ComponentLinksQuery> {
        let request_body = ComponentLinksQuery::build_query(component_links_query::Variables);
        self.query::<ComponentLinksQuery>(&request_body).await
    }
}

pub trait TestSubscriptionExt {
    fn errors_total_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ErrorsTotalSubscription>;

    fn component_errors_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentErrorsTotalsSubscription>;
}

impl TestSubscriptionExt for crate::SubscriptionClient {
    fn errors_total_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ErrorsTotalSubscription> {
        let request_body =
            ErrorsTotalSubscription::build_query(errors_total_subscription::Variables { interval });

        self.start::<ErrorsTotalSubscription>(&request_body)
    }

    fn component_errors_totals_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentErrorsTotalsSubscription> {
        let request_body = ComponentErrorsTotalsSubscription::build_query(
            component_errors_totals_subscription::Variables { interval },
        );

        self.start::<ComponentErrorsTotalsSubscription>(&request_body)
    }
}
