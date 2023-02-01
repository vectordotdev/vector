use async_trait::async_trait;
use graphql_client::GraphQLQuery;

use crate::{BoxedSubscription, QueryResult};

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

/// File source metrics query
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "tests/queries/file_source_metrics.graphql",
    response_derives = "Debug"
)]
pub struct FileSourceMetricsQuery;

/// Component by id query
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "tests/queries/component_by_component_key.graphql",
    response_derives = "Debug"
)]
pub struct ComponentByComponentKeyQuery;

/// Component by id query
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "tests/queries/components_connection.graphql",
    response_derives = "Debug"
)]
pub struct ComponentsConnectionQuery;

#[async_trait]
pub trait TestQueryExt {
    async fn component_links_query(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i64>,
        last: Option<i64>,
    ) -> crate::QueryResult<ComponentLinksQuery>;
    async fn file_source_metrics_query(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i64>,
        last: Option<i64>,
    ) -> crate::QueryResult<FileSourceMetricsQuery>;
    async fn component_by_component_key_query(
        &self,
        component_id: &str,
    ) -> crate::QueryResult<ComponentByComponentKeyQuery>;
    async fn components_connection_query(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i64>,
        last: Option<i64>,
    ) -> crate::QueryResult<ComponentsConnectionQuery>;
}

#[async_trait]
impl TestQueryExt for crate::Client {
    async fn component_links_query(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i64>,
        last: Option<i64>,
    ) -> QueryResult<ComponentLinksQuery> {
        let request_body = ComponentLinksQuery::build_query(component_links_query::Variables {
            after,
            before,
            first,
            last,
        });
        self.query::<ComponentLinksQuery>(&request_body).await
    }

    async fn file_source_metrics_query(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i64>,
        last: Option<i64>,
    ) -> QueryResult<FileSourceMetricsQuery> {
        let request_body =
            FileSourceMetricsQuery::build_query(file_source_metrics_query::Variables {
                after,
                before,
                first,
                last,
            });
        self.query::<FileSourceMetricsQuery>(&request_body).await
    }

    async fn component_by_component_key_query(
        &self,
        component_id: &str,
    ) -> QueryResult<ComponentByComponentKeyQuery> {
        let request_body = ComponentByComponentKeyQuery::build_query(
            component_by_component_key_query::Variables {
                component_id: component_id.to_string(),
            },
        );
        self.query::<ComponentByComponentKeyQuery>(&request_body)
            .await
    }

    async fn components_connection_query(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i64>,
        last: Option<i64>,
    ) -> QueryResult<ComponentsConnectionQuery> {
        let request_body =
            ComponentsConnectionQuery::build_query(components_connection_query::Variables {
                after,
                before,
                first,
                last,
            });
        self.query::<ComponentsConnectionQuery>(&request_body).await
    }
}

pub trait TestSubscriptionExt {
    fn errors_total_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ErrorsTotalSubscription>;
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
}
