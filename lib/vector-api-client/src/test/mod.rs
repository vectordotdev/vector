use crate::QueryResult;
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

/// File source metrics query
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "tests/queries/file_source_metrics.graphql",
    response_derives = "Debug"
)]
pub struct FileSourceMetricsQuery;

#[async_trait]
pub trait TestQueryExt {
    async fn component_links_query(&self) -> crate::QueryResult<ComponentLinksQuery>;
    async fn file_source_metrics_query(&self) -> crate::QueryResult<FileSourceMetricsQuery>;
}

#[async_trait]
impl TestQueryExt for crate::Client {
    async fn component_links_query(&self) -> QueryResult<ComponentLinksQuery> {
        let request_body = ComponentLinksQuery::build_query(component_links_query::Variables);
        self.query::<ComponentLinksQuery>(&request_body).await
    }

    async fn file_source_metrics_query(&self) -> QueryResult<FileSourceMetricsQuery> {
        let request_body =
            FileSourceMetricsQuery::build_query(file_source_metrics_query::Variables);
        self.query::<FileSourceMetricsQuery>(&request_body).await
    }
}
