use async_trait::async_trait;
use graphql_client::GraphQLQuery;

/// MetaVersionStringQuery returns the version string of the queried Vector instance.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/queries/meta_version_string.graphql",
    response_derives = "Debug"
)]
pub struct MetaVersionStringQuery;

/// Extension methods for meta queries.
#[async_trait]
pub trait MetaQueryExt {
    /// Executes a meta version string query.
    async fn meta_version_string(&self) -> crate::QueryResult<MetaVersionStringQuery>;
}

#[async_trait]
impl MetaQueryExt for crate::Client {
    /// Executes a meta version string query.
    async fn meta_version_string(&self) -> crate::QueryResult<MetaVersionStringQuery> {
        self.query::<MetaVersionStringQuery>(&MetaVersionStringQuery::build_query(
            meta_version_string_query::Variables,
        ))
        .await
    }
}
