#[cfg(feature = "api")]
#[macro_use]
extern crate matches;

mod support;

#[cfg(all(feature = "api", feature = "vector-api-client"))]
mod tests {
    use crate::support::{sink, source};
    use chrono::Utc;
    use futures::StreamExt;
    use std::{
        net::SocketAddr,
        sync::Once,
        time::{Duration, Instant},
    };
    use tokio::{select, sync::oneshot};
    use url::Url;
    use vector::{
        self,
        api::{self, Server},
        config::Config,
        internal_events::{emit, GeneratorEventProcessed, Heartbeat},
        test_util::{next_addr, retry_until},
    };
    use vector_api_client::{
        connect_subscription_client,
        gql::{HealthQueryExt, HealthSubscriptionExt, MetricsSubscriptionExt},
        Client, SubscriptionClient,
    };

    static METRICS_INIT: Once = Once::new();

    // Initialize the metrics system. Idempotent.
    fn init_metrics() -> oneshot::Sender<()> {
        METRICS_INIT.call_once(|| {
            let _ = vector::metrics::init();
        });

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let since = Instant::now();
            let mut timer = tokio::time::interval(Duration::from_secs(1));

            loop {
                select! {
                    _ = &mut shutdown_rx => break,
                    _ = timer.tick() => {
                        emit(Heartbeat { since });
                    }
                }
            }
        });

        shutdown_tx
    }

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

    // Starts and returns the server
    fn start_server() -> Server {
        let config = api_enabled_config();
        api::Server::start(&config)
    }

    fn make_client(addr: SocketAddr) -> Client {
        let url = Url::parse(&*format!("http://{}/graphql", addr)).unwrap();

        Client::new(url)
    }

    // Returns the result of a URL test against the API. Wraps the test in retry_until
    // to guard against the race condition of the TCP listener not being ready
    async fn url_test(config: Config, url: &'static str) -> reqwest::Response {
        let addr = config.api.bind.unwrap();
        let url = format!("http://{}:{}/{}", addr.ip(), addr.port(), url);

        let _server = api::Server::start(&config);

        // Build the request
        let client = reqwest::Client::new();

        retry_until(
            || client.get(&url).send(),
            Duration::from_millis(100),
            Duration::from_secs(10),
        )
        .await
    }

    // Creates and returns a new subscription client. Connection is re-attempted until
    // the specified timeout
    async fn new_subscription_client(addr: SocketAddr) -> SubscriptionClient {
        let url = Url::parse(&*format!("ws://{}/graphql", addr)).unwrap();

        retry_until(
            || connect_subscription_client(&url),
            Duration::from_millis(50),
            Duration::from_secs(10),
        )
        .await
    }

    // Emits fake generate events every 10ms until the returned shutdown falls out of scope
    fn emit_fake_generator_events() -> oneshot::Sender<()> {
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(Duration::from_millis(10));

            loop {
                select! {
                    _ = &mut shutdown_rx => break,
                    _ = timer.tick() => {
                        emit(GeneratorEventProcessed);
                    }
                }
            }
        });

        shutdown_tx
    }

    async fn new_heartbeat_subscription(
        client: &SubscriptionClient,
        num_results: usize,
        interval: i64,
    ) {
        let subscription = client.heartbeat_subscription(interval).await.unwrap();

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

            assert!(diff.num_milliseconds() >= mul as i64 * interval);
        }

        // Stream should have stopped after `num_results`
        assert_matches!(heartbeats.next().await, None);
    }

    async fn new_uptime_subscription(client: &SubscriptionClient) {
        let subscription = client.uptime_metrics_subscription().await.unwrap();

        tokio::pin! {
            let uptime = subscription.stream().skip(1);
        }

        // Uptime should be above zero
        assert!(
            uptime
                .take(1)
                .next()
                .await
                .unwrap()
                .unwrap()
                .data
                .unwrap()
                .uptime_metrics
                .seconds
                > 0.00
        )
    }

    async fn new_events_processed_total_subscription(
        client: &SubscriptionClient,
        num_results: usize,
        interval: i64,
    ) {
        // Emit events for the duration of the test
        let _shutdown = emit_fake_generator_events();

        let subscription = client
            .events_processed_total_metrics_subscription(interval)
            .await
            .unwrap();

        tokio::pin! {
            let events_processed_total = subscription.stream().take(num_results);
        }

        let mut last_result = 0.0;

        for _ in 0..num_results {
            let ep = events_processed_total
                .next()
                .await
                .unwrap()
                .unwrap()
                .data
                .unwrap()
                .events_processed_total_metrics
                .events_processed_total;

            assert!(ep > last_result);
            last_result = ep
        }
    }

    #[tokio::test]
    /// Tests the /health endpoint returns a 200 responses (non-GraphQL)
    async fn api_health() {
        let res = url_test(api_enabled_config(), "health")
            .await
            .text()
            .await
            .unwrap();

        assert!(res.contains("ok"));
    }

    #[tokio::test]
    /// Tests that the API playground is enabled when playground = true (implicit)
    async fn api_playground_enabled() {
        let mut config = api_enabled_config();
        config.api.playground = true;

        let res = url_test(config, "playground").await.status();

        assert!(res.is_success());
    }

    #[tokio::test]
    /// Tests that the /playground URL is inaccessible if it's been explicitly disabled
    async fn api_playground_disabled() {
        let mut config = api_enabled_config();
        config.api.playground = false;

        let res = url_test(config, "playground").await.status();

        assert!(res.is_client_error());
    }

    #[tokio::test]
    /// Tests the health query
    async fn api_graphql_health() {
        let server = start_server();
        let client = make_client(server.addr());

        let res = client.health_query().await.unwrap();

        assert!(res.data.unwrap().health);
        assert_eq!(res.errors, None);
    }

    #[tokio::test]
    /// Tests that the heartbeat subscription returns a UTC payload every 1/2 second
    async fn api_graphql_heartbeat() {
        let server = start_server();
        let client = new_subscription_client(server.addr()).await;

        new_heartbeat_subscription(&client, 3, 500).await;
    }

    #[tokio::test]
    /// Tests for Vector instance uptime in seconds
    async fn api_graphql_uptime_metrics() {
        let server = start_server();
        let client = new_subscription_client(server.addr()).await;

        let _metrics = init_metrics();

        new_uptime_subscription(&client).await;
    }

    #[tokio::test]
    /// Tests for events processed metrics, using fake generator events
    async fn api_graphql_event_processed_total_metrics() {
        let server = start_server();
        let client = new_subscription_client(server.addr()).await;

        let _metrics = init_metrics();

        new_events_processed_total_subscription(&client, 3, 100).await;
    }

    #[tokio::test]
    /// Tests whether 2 disparate subscriptions can run against a single client
    async fn api_graphql_combined_heartbeat_uptime() {
        let server = start_server();
        let client = new_subscription_client(server.addr()).await;

        let _metrics = init_metrics();

        futures::join! {
            new_uptime_subscription(&client),
            new_heartbeat_subscription(&client, 3, 500),
        };
    }
}
