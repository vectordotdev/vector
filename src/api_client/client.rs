use graphql_client::GraphQLQuery;
use url::Url;

/// Executes a GraphQL query, and returns a JSON response typed to the `ResponseData`
/// that matches the GraphQL's `QueryBody` type
pub async fn query<T: GraphQLQuery>(
    url: Url,
    request_body: &graphql_client::QueryBody<T::Variables>,
) -> Result<graphql_client::Response<T::ResponseData>, reqwest::Error> {
    let client = reqwest::Client::new();

    client
        .post(url)
        .json(&request_body)
        .send()
        .await?
        .json()
        .await
}
