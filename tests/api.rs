mod support;

#[cfg(feature = "api")]
mod tests {
    use crate::support::{sink, source};
    use reqwest::Response;
    use std::time::Duration;
    use vector::api;
    use vector::config::Config;
    use vector::test_util::{next_addr, retry_until};

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
    async fn url_test(config: Config, url: &'static str) -> Response {
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
}
