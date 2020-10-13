use anyhow::Context;
use graphql_client::GraphQLQuery;
use url::Url;

/// Wrapped `Result` type, that returns deserialized GraphQL response data
pub type QueryResult<T> =
    anyhow::Result<graphql_client::Response<<T as GraphQLQuery>::ResponseData>>;

/// GraphQL query client over HTTP
#[derive(Debug)]
pub struct Client {
    url: Url,
}

impl Client {
    /// Returns a new GraphQL query client, bound to the provided URL
    pub fn new(url: Url) -> Self {
        Self { url }
    }

    /// Issue a GraphQL query using Reqwest, serializing the response to the associated
    /// GraphQL type for the given `request_body`
    pub async fn query<T: GraphQLQuery>(
        &self,
        request_body: &graphql_client::QueryBody<T::Variables>,
    ) -> QueryResult<T> {
        let client = reqwest::Client::new();

        client
            .post(self.url.clone())
            .json(request_body)
            .send()
            .await
            .with_context(|| {
                format!(
                    "Couldn't send '{}' query to {}",
                    request_body.operation_name,
                    &self.url.as_str()
                )
            })?
            .json()
            .await
            .with_context(|| {
                format!(
                    "Couldn't serialize the response for '{}' query: {:?}",
                    request_body.operation_name, request_body.query
                )
            })
    }
}
