#[cfg(feature = "api")]
#[macro_use]
extern crate matches;

mod support;

#[cfg(all(feature = "api", feature = "vector-api-client"))]
mod tests {
    use crate::support::{sink, source_with_event_counter, transform};
    use chrono::Utc;
    use futures::StreamExt;
    use std::{
        net::SocketAddr,
        sync::Once,
        time::{Duration, Instant},
    };
    use tokio::sync::oneshot;
    use url::Url;
    use vector::{
        self,
        api::{self, Server},
        config::{self, Config, Format},
        internal_events::{emit, GeneratorEventProcessed, Heartbeat},
        test_util::{next_addr, retry_until},
    };
    use vector_api_client::{
        connect_subscription_client,
        gql::{
            ComponentsSubscriptionExt, HealthQueryExt, HealthSubscriptionExt, MetaQueryExt,
            MetricsSubscriptionExt,
        },
        test::*,
        Client, SubscriptionClient,
    };

    static METRICS_INIT: Once = Once::new();

    // Initialize the metrics system. Idempotent.
    fn init_metrics() -> oneshot::Sender<()> {
        METRICS_INIT.call_once(|| {
            vector::trace::init(true, true, "info");
            let _ = vector::metrics::init();
        });

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let since = Instant::now();
            tokio::time::interval(Duration::from_secs(1))
                .take_until(shutdown_rx)
                .for_each(|_| async move { emit(Heartbeat { since }) })
                .await
        });

        shutdown_tx
    }

    // Provides a config that enables the API server, assigned to a random port. Implicitly
    // tests that the config shape matches expectations
    fn api_enabled_config() -> Config {
        let mut config = Config::builder();
        config.add_source("in1", source_with_event_counter().1);
        config.add_sink("out1", &["in1"], sink(10).1);
        config.api.enabled = true;
        config.api.address = Some(next_addr());

        config.build().unwrap()
    }

    async fn from_str_config(
        conf: &str,
        format: config::FormatHint,
    ) -> vector::topology::RunningTopology {
        let mut c = config::load_from_str(conf, format).unwrap();
        c.api.address = Some(next_addr());

        let diff = config::ConfigDiff::initial(&c);
        let pieces = vector::topology::build_or_log_errors(&c, &diff)
            .await
            .unwrap();

        let result = vector::topology::start_validated(c, diff, pieces, false).await;
        let (topology, _graceful_crash) = result.unwrap();

        topology
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
        let addr = config.api.address.unwrap();
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
            || connect_subscription_client(url.clone()),
            Duration::from_millis(50),
            Duration::from_secs(10),
        )
        .await
    }

    // Emits fake generate events every 10ms until the returned shutdown falls out of scope
    fn emit_fake_generator_events() -> oneshot::Sender<()> {
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            tokio::time::interval(Duration::from_millis(10))
                .take_until(shutdown_rx)
                .for_each(|_| async { emit(GeneratorEventProcessed) })
                .await
        });

        shutdown_tx
    }

    async fn new_heartbeat_subscription(
        client: &SubscriptionClient,
        num_results: usize,
        interval: i64,
    ) {
        let subscription = client.heartbeat_subscription(interval);

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
        let subscription = client.uptime_subscription();

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
                .uptime
                .seconds
                > 0.00
        )
    }

    async fn new_processed_events_total_subscription(
        client: &SubscriptionClient,
        num_results: usize,
        interval: i64,
    ) {
        // Emit events for the duration of the test
        let _shutdown = emit_fake_generator_events();

        let subscription = client.processed_events_total_subscription(interval);

        tokio::pin! {
            let processed_events_total = subscription.stream().take(num_results);
        }

        let mut last_result = 0.0;

        for _ in 0..num_results {
            let ep = processed_events_total
                .next()
                .await
                .unwrap()
                .unwrap()
                .data
                .unwrap()
                .processed_events_total
                .processed_events_total;

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
    /// Tests links between components
    async fn test_component_links() {
        let mut config_builder = Config::builder();
        config_builder.add_source("in1", source_with_event_counter().1);
        config_builder.add_transform("t1", &["in1"], transform("t1_", 1.1));
        config_builder.add_transform("t2", &["t1"], transform("t2_", 1.1));
        config_builder.add_sink("out1", &["in1", "t2"], sink(10).1);
        config_builder.api.enabled = true;
        config_builder.api.address = Some(next_addr());

        let config = config_builder.build().unwrap();
        let server = api::Server::start(&config);

        let client = make_client(server.addr());

        let res = client.component_links_query().await.unwrap();
        let data = res.data.unwrap();

        // should be a single source named "in1"
        assert!(data.sources.len() == 1);
        assert!(data.sources[0].name == "in1");

        // "in1" source should link to exactly one transform named "t1"
        assert!(data.sources[0].transforms.len() == 1);
        assert!(data.sources[0].transforms[0].name == "t1");

        // "in1" source should link to exactly one sink named "out2"
        assert!(data.sources[0].sinks.len() == 1);
        assert!(data.sources[0].sinks[0].name == "out1");

        // there should be 2 transforms
        assert!(data.transforms.len() == 2);

        // get a reference to "t1" and "t2"
        let mut t1 = &data.transforms[0];
        let mut t2 = &data.transforms[1];

        // swap if needed
        if t1.name == "t2" {
            t1 = &data.transforms[1];
            t2 = &data.transforms[0];
        }

        // "t1" transform should link to exactly one source named "in1"
        assert!(t1.sources.len() == 1);
        assert!(t1.sources[0].name == "in1");

        // "t1" transform should link to exactly one transform named "t2"
        assert!(t1.transforms.len() == 1);
        assert!(t1.transforms[0].name == "t2");

        // "t1" transform should NOT link to any sinks
        assert!(t1.sinks.is_empty());

        // "t2" transform should link to exactly one sink named "out1"
        assert!(t2.sinks.len() == 1);
        assert!(t2.sinks[0].name == "out1");

        // "t2" transform should NOT link to any sources or transforms
        assert!(t2.sources.is_empty());
        assert!(t2.transforms.is_empty());

        // should be a single sink named "out1"
        assert!(data.sinks.len() == 1);
        assert!(data.sinks[0].name == "out1");

        // "out1" sink should link to exactly one source named "in1"
        assert!(data.sinks[0].sources.len() == 1);
        assert!(data.sinks[0].sources[0].name == "in1");

        // "out1" sink should link to exactly one transform named "t2"
        assert!(data.sinks[0].transforms.len() == 1);
        assert!(data.sinks[0].transforms[0].name == "t2");

        assert_eq!(res.errors, None);
    }

    #[tokio::test]
    /// tests that version_string meta matches the current Vector version
    async fn api_graphql_meta_version_string() {
        let server = start_server();
        let client = make_client(server.addr());

        let res = client.meta_version_string().await.unwrap();

        assert_eq!(res.data.unwrap().meta.version_string, vector::get_version());
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

        new_processed_events_total_subscription(&client, 3, 100).await;
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

    #[tokio::test]
    #[allow(clippy::float_cmp)]
    #[ignore]
    /// Tests componentProcessedEventsTotals returns increasing metrics, ordered by
    /// source -> transform -> sink
    async fn api_graphql_component_processed_events_totals() {
        init_metrics();

        let topology = from_str_config(
            r#"
            [api]
              enabled = true

            [sources.processed_events_total_batch_source]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sinks.processed_events_total_batch_sink]
              # General
              type = "blackhole"
              inputs = ["processed_events_total_batch_source"]
              print_amount = 100000
            "#,
            Some(Format::TOML),
        )
        .await;

        let server = api::Server::start(topology.config());
        let client = new_subscription_client(server.addr()).await;
        let subscription = client.component_processed_events_totals_subscription(500);

        tokio::pin! {
            let stream = subscription.stream();
        }

        let data = stream
            .next()
            .await
            .unwrap()
            .unwrap()
            .data
            .unwrap()
            .component_processed_events_totals;

        assert_eq!(data[0].name, "processed_events_total_batch_source");
        assert_eq!(data[1].name, "processed_events_total_batch_sink");

        assert_eq!(
            data[0].metric.processed_events_total,
            data[1].metric.processed_events_total
        );
    }

    #[tokio::test]
    #[allow(clippy::float_cmp)]
    #[ignore]
    /// Tests componentProcessedBytesTotals returns increasing metrics, ordered by
    /// source -> transform -> sink
    async fn api_graphql_component_processed_bytes_totals() {
        init_metrics();

        let topology = from_str_config(
            r#"
            [api]
              enabled = true

            [sources.processed_bytes_total_batch_source]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sinks.processed_bytes_total_batch_sink]
              # General
              type = "blackhole"
              inputs = ["processed_bytes_total_batch_source"]
              print_amount = 100000
            "#,
            Some(Format::TOML),
        )
        .await;

        let server = api::Server::start(topology.config());
        let client = new_subscription_client(server.addr()).await;
        let subscription = client.component_processed_bytes_totals_subscription(500);

        tokio::pin! {
            let stream = subscription.stream();
        }

        let data = stream
            .next()
            .await
            .unwrap()
            .unwrap()
            .data
            .unwrap()
            .component_processed_bytes_totals;

        // Bytes are currently only relevant on sinks
        assert_eq!(data[0].name, "processed_bytes_total_batch_sink");
        assert!(data[0].metric.processed_bytes_total > 0.00);
    }

    #[tokio::test]
    #[ignore]
    /// Tests componentAdded receives an added component
    async fn api_graphql_component_added_subscription() {
        init_metrics();

        // Initial topology
        let mut topology = from_str_config(
            r#"
            [api]
              enabled = true

            [sources.component_added_source_1]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sinks.component_added_sink]
              # General
              type = "blackhole"
              inputs = ["component_added_source_1"]
              print_amount = 100000
            "#,
            Some(Format::TOML),
        )
        .await;

        let server = api::Server::start(topology.config());
        let client = new_subscription_client(server.addr()).await;

        // Spawn a handler for listening to changes
        let handle = tokio::spawn(async move {
            let subscription = client.component_added();

            tokio::pin! {
                let component_added = subscription.stream();
            }

            assert_eq!(
                "component_added_source_2",
                component_added
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .data
                    .unwrap()
                    .component_added
                    .name,
            );
        });

        // After a short delay, update the config to include `gen2`
        tokio::time::delay_for(tokio::time::Duration::from_millis(200)).await;

        let c = config::load_from_str(
            r#"
            [api]
              enabled = true

            [sources.component_added_source_1]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sources.component_added_source_2]
              type = "generator"
              lines = ["3rd line", "4th line"]
              batch_interval = 0.1

            [sinks.component_added_sink]
              # General
              type = "blackhole"
              inputs = ["component_added_source_1", "component_added_source_2"]
              print_amount = 100000
            "#,
            Some(Format::TOML),
        )
        .unwrap();

        topology.reload_config_and_respawn(c, false).await.unwrap();
        server.update_config(topology.config());

        // Await the join handle
        handle.await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    /// Tests componentRemoves detects when a component has been removed
    async fn api_graphql_component_removed_subscription() {
        init_metrics();

        // Initial topology
        let mut topology = from_str_config(
            r#"
            [api]
              enabled = true

            [sources.component_removed_source_1]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sources.component_removed_source_2]
              type = "generator"
              lines = ["3rd line", "4th line"]
              batch_interval = 0.1

            [sinks.component_removed_sink]
              # General
              type = "blackhole"
              inputs = ["component_removed_source_1", "component_removed_source_2"]
              print_amount = 100000
            "#,
            Some(Format::TOML),
        )
        .await;

        let server = api::Server::start(topology.config());
        let client = new_subscription_client(server.addr()).await;

        // Spawn a handler for listening to changes
        let handle = tokio::spawn(async move {
            let subscription = client.component_removed();

            tokio::pin! {
                let component_removed = subscription.stream();
            }

            assert_eq!(
                "component_removed_source_2",
                component_removed
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .data
                    .unwrap()
                    .component_removed
                    .name,
            );
        });

        // After a short delay, update the config to remove `gen2`
        tokio::time::delay_for(tokio::time::Duration::from_millis(200)).await;

        let c = config::load_from_str(
            r#"
            [api]
              enabled = true

            [sources.component_removed_source_1]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sinks.component_removed_sink]
              # General
              type = "blackhole"
              inputs = ["component_removed_source_1"]
              print_amount = 100000
            "#,
            Some(Format::TOML),
        )
        .unwrap();

        topology.reload_config_and_respawn(c, false).await.unwrap();
        server.update_config(topology.config());

        // Await the join handle
        handle.await.unwrap();
    }
}
