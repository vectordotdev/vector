use crate::gql;
use anyhow::Context;
use graphql_client::GraphQLQuery;
use url::Url;

pub struct Client {
    url: Url,
}

impl Client {
    pub fn new(url: Url) -> Self {
        Self { url }
    }

    /// Issue a GraphQL query using Reqwest, serializing the response to the associated
    /// GraphQL type for the given `request_body`
    async fn query<T: GraphQLQuery>(
        &self,
        request_body: &graphql_client::QueryBody<T::Variables>,
    ) -> crate::Result<T> {
        let client = reqwest::Client::new();

        client
            .post(self.url.clone())
            .json(&request_body)
            .send()
            .await?
            .json()
            .await
            .with_context(|| format!("test"))
    }

    /// Health query. Typically used to assert the API server is alive
    pub async fn health(&self) -> crate::Result<gql::HealthQuery> {
        let request_body = gql::HealthQuery::build_query(gql::health_query::Variables);

        self.query::<gql::HealthQuery>(&request_body).await
    }
}
