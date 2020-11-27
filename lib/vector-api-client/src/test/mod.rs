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
