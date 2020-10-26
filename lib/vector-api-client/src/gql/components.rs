use crate::QueryResult;
use async_trait::async_trait;
use graphql_client::GraphQLQuery;
use serde::export::Formatter;

/// Components query for returning sources, transforms, and sinks
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/queries/components.graphql",
    response_derives = "Debug"
)]
pub struct ComponentsQuery;

#[async_trait]
pub trait ComponentsQueryExt {
    async fn components_query(&self) -> crate::QueryResult<ComponentsQuery>;
}

#[async_trait]
impl ComponentsQueryExt for crate::Client {
    async fn components_query(&self) -> QueryResult<ComponentsQuery> {
        let request_body = ComponentsQuery::build_query(components_query::Variables);
        self.query::<ComponentsQuery>(&request_body).await
    }
}

impl std::fmt::Display for components_query::ComponentsQueryComponentsOn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let res = match self {
            components_query::ComponentsQueryComponentsOn::Source => "source",
            components_query::ComponentsQueryComponentsOn::Transform => "transform",
            components_query::ComponentsQueryComponentsOn::Sink => "sink",
        };

        write!(f, "{}", res)
    }
}
