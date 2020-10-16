use crate::QueryResult;
use async_trait::async_trait;
use graphql_client::GraphQLQuery;
use serde::export::Formatter;

/// Topology query for returning sources, transforms and sinks
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/queries/topology.graphql",
    response_derives = "Debug"
)]
pub struct TopologyQuery;

#[async_trait]
pub trait TopologyQueryExt {
    async fn topology_query(&self) -> crate::QueryResult<TopologyQuery>;
}

#[async_trait]
impl TopologyQueryExt for crate::Client {
    async fn topology_query(&self) -> QueryResult<TopologyQuery> {
        let request_body = TopologyQuery::build_query(topology_query::Variables);
        self.query::<TopologyQuery>(&request_body).await
    }
}

impl std::fmt::Display for topology_query::TopologyQueryTopologyOn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let res = match self {
            topology_query::TopologyQueryTopologyOn::Source => "source",
            topology_query::TopologyQueryTopologyOn::Transform => "transform",
            topology_query::TopologyQueryTopologyOn::Sink => "sink",
        };

        write!(f, "{}", res)
    }
}
