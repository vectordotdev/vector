#[cfg(feature = "api")]
#[macro_use]
extern crate matches;

mod support;

#[cfg(feature = "api")]
mod tests {
    use crate::support::{sink, source};
    use chrono::Utc;
    use futures::StreamExt;
    use graphql_client::*;
    use std::time::Duration;
    use vector::api;
    use vector::config::Config;
    use vector::test_util::{next_addr, retry_until};

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "graphql/schema.json",
        query_path = "graphql/queries/health.graphql",
        response_derives = "Debug"
    )]
    struct HealthQuery;

    type DateTime = chrono::DateTime<chrono::Utc>;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "graphql/schema.json",
        query_path = "graphql/subscriptions/heartbeat.graphql",
        response_derives = "Debug"
    )]
    struct HeartbeatSubscription;

    // Provides a config that enables the API server, assigned to a random port. Implicitly
    // tests that the config shape matches expectations
    fn api_enabled_config() -> Config {
        let mut config = Config::builder();
        config.add_source("in1", source().1);
        config.add_sink("out1", &["in1"], sink(10).1);
        config.api.enabled = true;
        config.api.bind = Some(next_addr());

        config.build().unwrap()
    }

    // Returns the result of a URL test against the API. Wraps the test in retry_until
    // to guard against the race condition of the TCP listener not being ready
    async fn url_test(config: Config, url: &'static str) -> reqwest::Response {
        let addr = config.api.bind.unwrap();
        let url = format!("http://{}:{}/{}", addr.ip(), addr.port(), url);

        let _server = api::Server::start(config.api);

        // Build the request
        let client = reqwest::Client::new();

        retry_until(
            || client.get(&url).send(),
            Duration::from_millis(100),
            Duration::from_secs(10),
        )
        .await
    }

    async fn query<T: GraphQLQuery>(
        request_body: &graphql_client::QueryBody<T::Variables>,
    ) -> graphql_client::Response<T::ResponseData> {
        let config = api_enabled_config();
        let addr = config.api.bind.unwrap();
        let url = format!("http://{}:{}/graphql", addr.ip(), addr.port());

        let _server = api::Server::start(config.api);
        let client = reqwest::Client::new();

        retry_until(
            || client.post(&url).json(&request_body).send(),
            Duration::from_millis(100),
            Duration::from_secs(10),
        )
        .await
        .json()
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn api_health() {
        let res = url_test(api_enabled_config(), "health")
            .await
            .text()
            .await
            .unwrap();

        assert!(res.contains("ok"));
    }

    #[tokio::test]
    async fn api_playground_enabled() {
        let mut config = api_enabled_config();
        config.api.playground = true;

        let res = url_test(config, "playground").await.status();

        assert!(res.is_success());
    }

    #[tokio::test]
    async fn api_playground_disabled() {
        let mut config = api_enabled_config();
        config.api.playground = false;

        let res = url_test(config, "playground").await.status();

        assert!(res.is_client_error());
    }

    #[tokio::test]
    async fn api_graphql_health() {
        let request_body = HealthQuery::build_query(health_query::Variables);
        let res = query::<HealthQuery>(&request_body).await;

        assert_matches!(
            res,
            graphql_client::Response {
                data: Some(health_query::ResponseData { health: true }),
                errors: None,
            }
        );
    }

    #[tokio::test]
    async fn api_graphql_heartbeat() {
        let config = api_enabled_config();
        let _server = api::Server::start(config.api);
        let bind = config.api.bind.unwrap();
        let interval: i64 = 500;
        let num_results = 3;

        let request_body =
            HeartbeatSubscription::build_query(heartbeat_subscription::Variables { interval });

        let mut client = retry_until(
            || api::make_subscription_client(bind),
            Duration::from_millis(50),
            Duration::from_secs(10),
        )
        .await;

        let subscription = client
            .start::<HeartbeatSubscription>(&request_body)
            .await
            .unwrap();

        tokio::pin! {
            let heartbeats = subscription.stream().take(num_results);
        }

        // Should get 3x timestamps that are at least `interval` apart. The first one
        // will be almost immediate, so move it by `interval` to account for the diff
        let now = Utc::now() - chrono::Duration::milliseconds(interval);

        for mul in 1..=num_results {
            let diff = heartbeats
                .next()
                .await
                .unwrap()
                .unwrap()
                .data
                .unwrap()
                .heartbeat
                .utc
                - now;

            assert!(diff.num_milliseconds() > mul as i64 * interval);
        }

        // Stream should have stopped after `num_results`
        assert_matches!(heartbeats.next().await, None);
    }
}
