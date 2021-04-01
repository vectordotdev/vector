#[cfg(feature = "api")]
#[macro_use]
extern crate matches;

mod support;

#[cfg(all(feature = "api", feature = "vector-api-client"))]
mod tests {
    use crate::support::{sink, source_with_event_counter, transform};
    use chrono::Utc;
    use futures::StreamExt;
    use metrics::counter;
    use serial_test::serial;
    use std::{
        collections::HashMap,
        net::SocketAddr,
        time::{Duration, Instant},
    };
    use tokio::sync::oneshot;
    use tokio_stream::wrappers::IntervalStream;
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

    // Initialize the metrics system.
    fn init_metrics() -> oneshot::Sender<()> {
        println!("--- init metrics");
        vector::trace::init(true, true, "info");
        let _ = vector::metrics::init();

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let since = Instant::now();
            IntervalStream::new(tokio::time::interval(Duration::from_secs(1)))
                .take_until(shutdown_rx)
                .for_each(|_| async move { emit(Heartbeat { since }) })
                .await
        });

        shutdown_tx
    }

    fn reset_metrics() {
        println!("--- reset metrics");
        vector::trace::reset();
        vector::metrics::reset();
    }

    /// Invokes `fork_test`, and initializes metrics
    async fn metrics_test<T: std::future::Future>(fut: T) {
        reset_metrics();
        let _metrics = init_metrics();

        struct ResetMetrics;
        impl Drop for ResetMetrics {
            fn drop(&mut self) {
                reset_metrics();
            }
        }

        let _reset_metrics = ResetMetrics;

        fut.await;
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

    async fn from_str_config(conf: &str) -> vector::topology::RunningTopology {
        let mut c = config::load_from_str(conf, Some(Format::TOML)).unwrap();
        c.api.address = Some(next_addr());

        let diff = config::ConfigDiff::initial(&c);
        let pieces = vector::topology::build_or_log_errors(&c, &diff, HashMap::new())
            .await
            .unwrap();

        let result = vector::topology::start_validated(c, diff, pieces).await;
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
        println!("url_test 0");
        let addr = config.api.address.unwrap();
        let url = format!("http://{}:{}/{}", addr.ip(), addr.port(), url);

        let _server = api::Server::start(&config);
        println!("url_test 1");

        // Build the request
        let client = reqwest::Client::new();

        let response = retry_until(
            || client.get(&url).send(),
            Duration::from_millis(100),
            Duration::from_secs(10),
        )
        .await;
        println!("url_test 2 END");

        response
    }

    // Creates and returns a new subscription client. Connection is re-attempted until
    // the specified timeout
    async fn new_subscription_client(addr: SocketAddr) -> SubscriptionClient {
        println!("new_subscription_client 0");
        let url = Url::parse(&*format!("ws://{}/graphql", addr)).unwrap();

        let response = retry_until(
            || connect_subscription_client(url.clone()),
            Duration::from_millis(50),
            Duration::from_secs(10),
        )
        .await;

        println!("new_subscription_client 1 END");

        response
    }

    // Emits fake generate events every 10ms until the returned shutdown falls out of scope
    fn emit_fake_generator_events() -> oneshot::Sender<()> {
        println!("emit_fake_generator_events 0");
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            println!("emit_fake_generator_events 1");
            IntervalStream::new(tokio::time::interval(Duration::from_millis(10)))
                .take_until(shutdown_rx)
                .for_each(|_| async { emit(GeneratorEventProcessed) })
                .await;
            println!("emit_fake_generator_events 2");
        });

        println!("emit_fake_generator_events 3");

        let sender = shutdown_tx;

        println!("emit_fake_generator_events 4 END");

        sender
    }

    async fn new_heartbeat_subscription(
        client: &SubscriptionClient,
        num_results: usize,
        interval: i64,
    ) {
        println!("new_heartbeat_subscription 0");
        let subscription = client.heartbeat_subscription(interval);
        println!("new_heartbeat_subscription 1");

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

            println!("new_heartbeat_subscription 2 {}/{}", mul, num_results);

            assert!(diff.num_milliseconds() >= mul as i64 * interval);
        }

        println!("new_heartbeat_subscription 3");

        // Stream should have stopped after `num_results`
        assert_matches!(heartbeats.next().await, None);

        println!("new_heartbeat_subscription 4 END");
    }

    async fn new_uptime_subscription(client: &SubscriptionClient) {
        println!("new_uptime_subscription 0");
        let subscription = client.uptime_subscription();
        println!("new_uptime_subscription 1");

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
        );

        println!("new_uptime_subscription 2 END");
    }

    async fn new_processed_events_total_subscription(
        client: &SubscriptionClient,
        num_results: usize,
        interval: i64,
    ) {
        println!("new_processed_events_total_subscription 0");
        // Emit events for the duration of the test
        let _shutdown = emit_fake_generator_events();
        println!("new_processed_events_total_subscription 1");

        let subscription = client.processed_events_total_subscription(interval);
        println!("new_processed_events_total_subscription 2");

        tokio::pin! {
            let processed_events_total = subscription.stream().take(num_results);
        }

        let mut last_result = 0.0;

        for i in 0..num_results {
            let ep = processed_events_total
                .next()
                .await
                .unwrap()
                .unwrap()
                .data
                .unwrap()
                .processed_events_total
                .processed_events_total;

            println!(
                "new_processed_events_total_subscription 3 {}/{}",
                i, num_results
            );

            assert!(ep > last_result);
            last_result = ep;

            println!("new_processed_events_total_subscription 4");
        }

        println!("new_processed_events_total_subscription 5 END");
    }

    #[tokio::test]
    #[serial]
    /// Tests the /health endpoint returns a 200 responses (non-GraphQL)
    async fn api_health() {
        println!("api_health 0");
        let res = url_test(api_enabled_config(), "health")
            .await
            .text()
            .await
            .unwrap();

        println!("api_health 1");

        assert!(res.contains("ok"));

        println!("api_health 2 END");
    }

    #[tokio::test]
    #[serial]
    /// Tests that the API playground is enabled when playground = true (implicit)
    async fn api_playground_enabled() {
        println!("api_playground_enabled 0");
        let mut config = api_enabled_config();
        config.api.playground = true;
        println!("api_playground_enabled 1");

        let res = url_test(config, "playground").await.status();
        println!("api_playground_enabled 2");

        assert!(res.is_success());

        println!("api_playground_enabled 3 END");
    }

    #[tokio::test]
    #[serial]
    /// Tests that the /playground URL is inaccessible if it's been explicitly disabled
    async fn api_playground_disabled() {
        println!("api_playground_disabled 0");
        let mut config = api_enabled_config();
        config.api.playground = false;
        println!("api_playground_disabled 1");

        let res = url_test(config, "playground").await.status();
        println!("api_playground_disabled 2");

        assert!(res.is_client_error());
        println!("api_playground_disabled 3 END");
    }

    #[tokio::test]
    #[serial]
    /// Tests the health query
    async fn api_graphql_health() {
        println!("api_graphql_health 0");
        let server = start_server();
        let client = make_client(server.addr());
        println!("api_graphql_health 1");

        let res = client.health_query().await.unwrap();
        println!("api_graphql_health 2");

        assert!(res.data.unwrap().health);
        assert_eq!(res.errors, None);

        println!("api_graphql_health 3 END");
    }

    #[tokio::test]
    #[serial]
    /// Tests links between components
    async fn api_graphql_component_links() {
        println!("api_graphql_component_links 0");
        let mut config_builder = Config::builder();
        config_builder.add_source("in1", source_with_event_counter().1);
        config_builder.add_transform("t1", &["in1"], transform("t1_", 1.1));
        config_builder.add_transform("t2", &["t1"], transform("t2_", 1.1));
        config_builder.add_sink("out1", &["in1", "t2"], sink(10).1);
        config_builder.api.enabled = true;
        config_builder.api.address = Some(next_addr());

        let config = config_builder.build().unwrap();
        println!("api_graphql_component_links 1");
        let server = api::Server::start(&config);
        println!("api_graphql_component_links 2");

        let client = make_client(server.addr());
        println!("api_graphql_component_links 3");

        let res = client
            .component_links_query(None, None, None, None)
            .await
            .unwrap();
        println!("api_graphql_component_links 4");

        let data = res.data.unwrap();
        let sources = data
            .sources
            .edges
            .into_iter()
            .flatten()
            .flatten()
            .collect::<Vec<_>>();

        let transforms = data
            .transforms
            .edges
            .into_iter()
            .flatten()
            .flatten()
            .collect::<Vec<_>>();

        let sinks = data
            .sinks
            .edges
            .into_iter()
            .flatten()
            .flatten()
            .collect::<Vec<_>>();

        println!("api_graphql_component_links 5");

        // should be a single source named "in1"
        assert!(sources.len() == 1);
        assert!(sources[0].node.name == "in1");

        // "in1" source should link to exactly one transform named "t1"
        assert!(sources[0].node.transforms.len() == 1);
        assert!(sources[0].node.transforms[0].name == "t1");

        // "in1" source should link to exactly one sink named "out2"
        assert!(sources[0].node.sinks.len() == 1);
        assert!(sources[0].node.sinks[0].name == "out1");

        // there should be 2 transforms
        assert!(transforms.len() == 2);

        // get a reference to "t1" and "t2"
        let mut t1 = &transforms[0];
        let mut t2 = &transforms[1];

        // swap if needed
        if t1.node.name == "t2" {
            t1 = &transforms[1];
            t2 = &transforms[0];
        }

        // "t1" transform should link to exactly one source named "in1"
        assert!(t1.node.sources.len() == 1);
        assert!(t1.node.sources[0].name == "in1");

        // "t1" transform should link to exactly one transform named "t2"
        assert!(t1.node.transforms.len() == 1);
        assert!(t1.node.transforms[0].name == "t2");

        // "t1" transform should NOT link to any sinks
        assert!(t1.node.sinks.is_empty());

        // "t2" transform should link to exactly one sink named "out1"
        assert!(t2.node.sinks.len() == 1);
        assert!(t2.node.sinks[0].name == "out1");

        // "t2" transform should NOT link to any sources or transforms
        assert!(t2.node.sources.is_empty());
        assert!(t2.node.transforms.is_empty());

        // should be a single sink named "out1"
        assert!(sinks.len() == 1);
        assert!(sinks[0].node.name == "out1");

        // "out1" sink should link to exactly one source named "in1"
        assert!(sinks[0].node.sources.len() == 1);
        assert!(sinks[0].node.sources[0].name == "in1");

        // "out1" sink should link to exactly one transform named "t2"
        assert!(sinks[0].node.transforms.len() == 1);
        assert!(sinks[0].node.transforms[0].name == "t2");

        assert_eq!(res.errors, None);

        println!("api_graphql_component_links 6 END");
    }

    #[tokio::test]
    #[serial]
    /// tests that version_string meta matches the current Vector version
    async fn api_graphql_meta_version_string() {
        let server = start_server();
        let client = make_client(server.addr());

        let res = client.meta_version_string().await.unwrap();

        assert_eq!(res.data.unwrap().meta.version_string, vector::get_version());
    }

    #[tokio::test]
    #[serial]
    /// Tests that the heartbeat subscription returns a UTC payload every 1/2 second
    async fn api_graphql_heartbeat() {
        metrics_test(async {
            println!("api_graphql_heartbeat 0");
            let server = start_server();
            println!("api_graphql_heartbeat 1");
            let client = new_subscription_client(server.addr()).await;
            println!("api_graphql_heartbeat 2");

            new_heartbeat_subscription(&client, 3, 500).await;
            println!("api_graphql_heartbeat 3 END");
        })
        .await;
    }

    #[tokio::test]
    #[serial]
    /// Tests for Vector instance uptime in seconds
    async fn api_graphql_uptime_metrics() {
        metrics_test(async {
            println!("api_graphql_uptime_metrics 0");
            let server = start_server();
            println!("api_graphql_uptime_metrics 1");
            let client = new_subscription_client(server.addr()).await;
            println!("api_graphql_uptime_metrics 2");

            new_uptime_subscription(&client).await;
            println!("api_graphql_uptime_metrics 3 END");
        })
        .await;
    }

    #[tokio::test]
    #[serial]
    /// Tests for events processed metrics, using fake generator events
    async fn api_graphql_event_processed_total_metrics() {
        metrics_test(async {
            println!("api_graphql_event_processed_total_metrics 0");
            let server = start_server();
            println!("api_graphql_event_processed_total_metrics 1");
            let client = new_subscription_client(server.addr()).await;
            println!("api_graphql_event_processed_total_metrics 2");

            new_processed_events_total_subscription(&client, 3, 100).await;
            println!("api_graphql_event_processed_total_metrics 3 END");
        })
        .await;
    }

    #[tokio::test]
    #[serial]
    /// Tests whether 2 disparate subscriptions can run against a single client
    async fn api_graphql_combined_heartbeat_uptime() {
        metrics_test(async {
            println!("api_graphql_combined_heartbeat_uptime 0");
            let server = start_server();
            println!("api_graphql_combined_heartbeat_uptime 1");
            let client = new_subscription_client(server.addr()).await;
            println!("api_graphql_combined_heartbeat_uptime 2");

            futures::join! {
                new_uptime_subscription(&client),
                new_heartbeat_subscription(&client, 3, 500),
            };

            println!("api_graphql_combined_heartbeat_uptime 3 END");
        })
        .await;
    }

    #[tokio::test]
    #[serial]
    #[allow(clippy::float_cmp)]
    /// Tests componentProcessedEventsTotals returns increasing metrics, ordered by
    /// source -> transform -> sink
    async fn api_graphql_component_processed_events_totals() {
        metrics_test(async {
            println!("api_graphql_component_processed_events_totals 0");
            let conf = r#"
                    [api]
                      enabled = true

                    [sources.processed_events_total_batch_source]
                      type = "generator"
                      format = "shuffle"
                      lines = ["Random line", "And another"]
                      interval = 0.01

                    [sinks.processed_events_total_batch_sink]
                      # General
                      type = "blackhole"
                      inputs = ["processed_events_total_batch_source"]
                      print_amount = 100000
                "#;

            let topology = from_str_config(conf).await;
            println!("api_graphql_component_processed_events_totals 1");

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            println!("api_graphql_component_processed_events_totals 2");

            let server = api::Server::start(topology.config());
            println!("api_graphql_component_processed_events_totals 3");
            let client = new_subscription_client(server.addr()).await;
            println!("api_graphql_component_processed_events_totals 4");
            let subscription = client.component_processed_events_totals_subscription(500);
            println!("api_graphql_component_processed_events_totals 5");

            let data = subscription
                .stream()
                .skip(1)
                .take(1)
                .map(|r| r.unwrap().data.unwrap().component_processed_events_totals)
                .next()
                .await
                .expect("Didn't return results");
            println!("api_graphql_component_processed_events_totals 6");

            for name in &[
                "processed_events_total_batch_source",
                "processed_events_total_batch_sink",
            ] {
                assert!(data.iter().any(|d| d.name == *name));
            }

            println!("api_graphql_component_processed_events_totals 7 END");
        })
        .await;
    }

    #[tokio::test]
    #[serial]
    #[allow(clippy::float_cmp)]
    /// Tests componentProcessedBytesTotals returns increasing metrics, ordered by
    /// source -> transform -> sink
    async fn api_graphql_component_processed_bytes_totals() {
        metrics_test(async {
            println!("api_graphql_component_processed_bytes_totals 0");
            let conf = r#"
                    [api]
                      enabled = true

                    [sources.processed_bytes_total_batch_source]
                      type = "generator"
                      format = "shuffle"
                      lines = ["Random line", "And another"]
                      interval = 0.1

                    [sinks.processed_bytes_total_batch_sink]
                      # General
                      type = "blackhole"
                      inputs = ["processed_bytes_total_batch_source"]
                      print_amount = 100000
                "#;

            let topology = from_str_config(conf).await;
            println!("api_graphql_component_processed_bytes_totals 1");

            let server = api::Server::start(topology.config());
            println!("api_graphql_component_processed_bytes_totals 2");
            let client = new_subscription_client(server.addr()).await;
            println!("api_graphql_component_processed_bytes_totals 3");
            let subscription = client.component_processed_bytes_totals_subscription(500);
            println!("api_graphql_component_processed_bytes_totals 4");

            let data = subscription
                .stream()
                .skip(1)
                .take(1)
                .map(|r| r.unwrap().data.unwrap().component_processed_bytes_totals)
                .next()
                .await
                .expect("Didn't return results");
            println!("api_graphql_component_processed_bytes_totals 5");

            // Bytes are currently only relevant on sinks
            assert_eq!(data[0].name, "processed_bytes_total_batch_sink");
            assert!(data[0].metric.processed_bytes_total > 0.00);

            println!("api_graphql_component_processed_bytes_totals 6 END");
        })
        .await;
    }

    #[tokio::test]
    #[serial]
    /// Tests componentAdded receives an added component
    async fn api_graphql_component_added_subscription() {
        metrics_test(async {
            println!("api_graphql_component_added_subscription 0");
            let conf = r#"
                [api]
                  enabled = true

                [sources.component_added_source_1]
                  type = "generator"
                  format = "shuffle"
                  lines = ["Random line", "And another"]
                  interval = 0.1

                [sinks.component_added_sink]
                  # General
                  type = "blackhole"
                  inputs = ["component_added_source_1"]
                  print_amount = 100000
            "#;

            let mut topology = from_str_config(conf).await;
            println!("api_graphql_component_added_subscription 1");

            let server = api::Server::start(topology.config());
            println!("api_graphql_component_added_subscription 2");
            let client = new_subscription_client(server.addr()).await;
            println!("api_graphql_component_added_subscription 3");

            // Spawn a handler for listening to changes
            let handle = tokio::spawn(async move {
                let subscription = client.component_added();

                println!("api_graphql_component_added_subscription 4");

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

                println!("api_graphql_component_added_subscription 5");
            });

            // After a short delay, update the config to include `gen2`
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            println!("api_graphql_component_added_subscription 6");

            let conf = r#"
                [api]
                  enabled = true

                [sources.component_added_source_1]
                  type = "generator"
                  format = "shuffle"
                  lines = ["Random line", "And another"]
                  interval = 0.1

                [sources.component_added_source_2]
                  type = "generator"
                  format = "shuffle"
                  lines = ["3rd line", "4th line"]
                  interval = 0.1

                [sinks.component_added_sink]
                  # General
                  type = "blackhole"
                  inputs = ["component_added_source_1", "component_added_source_2"]
                  print_amount = 100000
            "#;

            let c = config::load_from_str(conf, Some(Format::TOML)).unwrap();
            println!("api_graphql_component_added_subscription 7");

            topology.reload_config_and_respawn(c).await.unwrap();
            println!("api_graphql_component_added_subscription 8");
            server.update_config(topology.config());
            println!("api_graphql_component_added_subscription 9");

            // Await the join handle
            handle.await.unwrap();
            println!("api_graphql_component_added_subscription 10 END");
        })
        .await;
    }

    #[tokio::test]
    #[serial]
    /// Tests componentRemoves detects when a component has been removed
    async fn api_graphql_component_removed_subscription() {
        metrics_test(async {
            println!("api_graphql_component_removed_subscription 0");
            let mut conf = r#"
                [api]
                  enabled = true

                [sources.component_removed_source_1]
                  type = "generator"
                  format = "shuffle"
                  lines = ["Random line", "And another"]
                  interval = 0.1

                [sources.component_removed_source_2]
                  type = "generator"
                  format = "shuffle"
                  lines = ["3rd line", "4th line"]
                  interval = 0.1

                [sinks.component_removed_sink]
                  # General
                  type = "blackhole"
                  inputs = ["component_removed_source_1", "component_removed_source_2"]
                  print_amount = 100000
            "#;

            let mut topology = from_str_config(conf).await;

            println!("api_graphql_component_removed_subscription 1");

            let server = api::Server::start(topology.config());
            let client = new_subscription_client(server.addr()).await;

            println!("api_graphql_component_removed_subscription 2");

            // Spawn a handler for listening to changes
            let handle = tokio::spawn(async move {
                println!("api_graphql_component_removed_subscription 3");

                let subscription = client.component_removed();

                tokio::pin! {
                    let component_removed = subscription.stream();
                }

                println!("api_graphql_component_removed_subscription 4");

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

                println!("api_graphql_component_removed_subscription 5");
            });

            println!("api_graphql_component_removed_subscription 6");

            // After a short delay, update the config to remove `gen2`
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

            println!("api_graphql_component_removed_subscription 7");

            // New configuration that will be reloaded
            conf = r#"
                [api]
                  enabled = true

                [sources.component_removed_source_1]
                  type = "generator"
                  format = "shuffle"
                  lines = ["Random line", "And another"]
                  interval = 0.1

                [sinks.component_removed_sink]
                  # General
                  type = "blackhole"
                  inputs = ["component_removed_source_1"]
                  print_amount = 100000
            "#;

            let c = config::load_from_str(conf, Some(Format::TOML)).unwrap();

            println!("api_graphql_component_removed_subscription 8");

            topology.reload_config_and_respawn(c).await.unwrap();

            println!("api_graphql_component_removed_subscription 9");

            server.update_config(topology.config());

            println!("api_graphql_component_removed_subscription 10");

            // Await the join handle
            handle.await.unwrap();

            println!("api_graphql_component_removed_subscription 11 END");
        })
        .await;
    }

    #[tokio::test]
    #[serial]
    async fn api_graphql_errors_total() {
        metrics_test(async {
            println!("api_graphql_errors_total 0");
            let conf = r#"
                [api]
                  enabled = true

                [sources.error_gen]
                  type = "generator"
                  format = "shuffle"
                  lines = ["Random line", "And another"]
                  batch_interval = 0.1

                [sinks.blackhole]
                  # General
                  type = "blackhole"
                  inputs = ["error_gen"]
                  print_amount = 100000
            "#;

            let topology = from_str_config(conf).await;
            println!("api_graphql_errors_total 1");

            let server = api::Server::start(topology.config());
            println!("api_graphql_errors_total 2");
            let client = new_subscription_client(server.addr()).await;
            println!("api_graphql_errors_total 3");

            // Spawn a handler for listening to changes
            let handle = tokio::spawn(async move {
                let subscription = client.errors_total_subscription(50);
                println!("api_graphql_errors_total 4");

                tokio::pin! {
                    let stream = subscription.stream();
                }

                // If we get results, it means the error has been picked up. Check the count is > 0
                assert!(
                    stream
                        .next()
                        .await
                        .unwrap()
                        .unwrap()
                        .data
                        .unwrap()
                        .errors_total
                        .errors_total
                        > 0.00
                );

                println!("api_graphql_errors_total 5");
            });

            // Emit an error metric
            counter!("processing_errors_total", 1);

            handle.await.unwrap();
            println!("api_graphql_errors_total 6 END");
        })
        .await;
    }

    #[tokio::test]
    #[serial]
    async fn api_grahql_component_errors_total() {
        metrics_test(async {
            println!("api_grahql_component_errors_total 0");
            let conf = r#"
                [api]
                  enabled = true

                [sources.error_gen]
                  type = "generator"
                  format = "shuffle"
                  lines = ["Random line", "And another"]
                  batch_interval = 0.1

                [sinks.blackhole]
                  # General
                  type = "blackhole"
                  inputs = ["error_gen"]
                  print_amount = 100000
            "#;

            let topology = from_str_config(conf).await;
            println!("api_grahql_component_errors_total 1");

            let server = api::Server::start(topology.config());
            println!("api_grahql_component_errors_total 2");
            let client = new_subscription_client(server.addr()).await;
            println!("api_grahql_component_errors_total 3");

            // Spawn a handler for listening to changes
            let handle = tokio::spawn(async move {
                println!("api_grahql_component_errors_total 4");
                let subscription = client.errors_total_subscription(50);
                println!("api_grahql_component_errors_total 5");

                tokio::pin! {
                    let stream = subscription.stream();
                }

                // If we get results, it means the error has been picked up. Check the count is > 0
                assert!(
                    stream
                        .next()
                        .await
                        .unwrap()
                        .unwrap()
                        .data
                        .unwrap()
                        .errors_total
                        .errors_total
                        > 0.00
                );

                println!("api_grahql_component_errors_total 6");
            });

            // Emit an error metric
            counter!("processing_errors_total", 1);

            handle.await.unwrap();
            println!("api_grahql_component_errors_total 7 END");
        })
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    #[serial]
    async fn api_graphql_files_source_metrics() {
        use std::io::Write;
        use tempfile::{tempdir, NamedTempFile};

        metrics_test(async {
            println!("api_graphql_files_source_metrics 0");
            let lines = vec!["test1", "test2", "test3"];

            let checkpoints = tempdir().unwrap();
            let mut named_file = NamedTempFile::new().unwrap();
            let path = named_file.path().to_str().unwrap().to_string();
            let mut file = named_file.as_file_mut();
            println!("api_graphql_files_source_metrics 1");

            for line in &lines {
                writeln!(&mut file, "{}", line).unwrap();
            }

            let conf = format!(
                r#"
                [api]
                  enabled = true

                [sources.file]
                  type = "file"
                  data_dir = "{}"
                  include = ["{}"]

                [sinks.out]
                  type = "blackhole"
                  inputs = ["file"]
                  print_amount = 100000
            "#,
                checkpoints.path().to_str().unwrap(),
                path
            );

            let topology = from_str_config(&conf).await;
            println!("api_graphql_files_source_metrics 2");
            let server = api::Server::start(topology.config());
            println!("api_graphql_files_source_metrics 3");

            // Short delay to ensure logs are picked up
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            println!("api_graphql_files_source_metrics 4");

            let client = make_client(server.addr());
            let res = client
                .file_source_metrics_query(None, None, None, None)
                .await;
            println!("api_graphql_files_source_metrics 5");

            match &res.unwrap().data.unwrap().sources.edges.into_iter().flatten().next().unwrap().unwrap().node.metrics.on {
                file_source_metrics_query::FileSourceMetricsQuerySourcesEdgesNodeMetricsOn::FileSourceMetrics(
                    file_source_metrics_query::FileSourceMetricsQuerySourcesEdgesNodeMetricsOnFileSourceMetrics { files, .. },
                ) => {
                    let node = &files.edges.iter().flatten().next().unwrap().as_ref().unwrap().node;
                    assert_eq!(node.name, path);
                    assert_eq!(node.processed_events_total.as_ref().unwrap().processed_events_total as usize, lines.len());
                }
                _ => panic!("not a file source"),
            };

            println!("api_graphql_files_source_metrics 6 END");
        }).await;
    }

    #[tokio::test]
    #[serial]
    async fn api_graphql_component_by_name() {
        metrics_test(async {
            println!("api_graphql_component_by_name 0");
            let conf = r#"
                [api]
                  enabled = true

                [sources.gen1]
                  type = "generator"
                  format = "shuffle"
                  lines = ["Random line", "And another"]
                  interval = 0.1

                [sinks.out]
                  type = "blackhole"
                  inputs = ["gen1"]
                  print_amount = 100000
            "#;

            let topology = from_str_config(&conf).await;
            println!("api_graphql_component_by_name 1");
            let server = api::Server::start(topology.config());
            println!("api_graphql_component_by_name 2");
            let client = make_client(server.addr());
            println!("api_graphql_component_by_name 3");

            // Retrieving a component that doesn't exist should return None
            let res = client.component_by_name_query("xxx").await;
            println!("api_graphql_component_by_name 4");
            assert!(res.unwrap().data.unwrap().component_by_name.is_none());
            println!("api_graphql_component_by_name 5");

            // The `gen1` name should exist
            let res = client.component_by_name_query("gen1").await;
            println!("api_graphql_component_by_name 6");
            assert_eq!(
                res.unwrap().data.unwrap().component_by_name.unwrap().name,
                "gen1"
            );
            println!("api_graphql_component_by_name 7 END");
        })
        .await;
    }

    #[tokio::test]
    #[serial]
    async fn api_graphql_components_connection() {
        metrics_test(async {
            println!("api_graphql_components_connection 0");
            // Config with a total of 5 components
            let conf = r#"
                [api]
                  enabled = true

                [sources.gen1]
                  type = "generator"
                  format = "shuffle"
                  lines = ["1"]
                  interval = 0.1

                [sources.gen2]
                  type = "generator"
                  format = "shuffle"
                  lines = ["2"]
                  interval = 0.1

                [sources.gen3]
                  type = "generator"
                  format = "shuffle"
                  lines = ["3"]
                  interval = 0.1

                [sources.gen4]
                  type = "generator"
                  format = "shuffle"
                  lines = ["4"]
                  interval = 0.1

                [sinks.out]
                  type = "blackhole"
                  inputs = ["gen1", "gen2", "gen3", "gen4"]
                  print_amount = 100000
            "#;

            let topology = from_str_config(&conf).await;
            println!("api_graphql_components_connection 1");

            let server = api::Server::start(topology.config());
            println!("api_graphql_components_connection 2");
            let client = make_client(server.addr());
            println!("api_graphql_components_connection 3");

            // Test after/first with a page size of 2, exhausting all results
            let mut cursor: Option<String> = None;
            for i in 0..3 {
                // The components connection contains a `pageInfo` and `edges` -- we need
                // both to assertion the result set matches expectations
                let components = client
                    .components_connection_query(cursor.clone(), None, Some(2), None)
                    .await
                    .unwrap()
                    .data
                    .unwrap()
                    .components;

                println!("api_graphql_components_connection 4-{}", i);

                // Total count should match the # of components
                assert_eq!(components.total_count, 5);

                let page_info = components.page_info;

                // Check prev/next paging is accurate
                assert_eq!(
                    (page_info.has_previous_page, page_info.has_next_page),
                    match i {
                        0 => (false, true),
                        2 => (true, false),
                        _ => (true, true),
                    }
                );

                // The # of `edges` results should be 2, except the last page
                assert_eq!(
                    components.edges.iter().flatten().count(),
                    match i {
                        2 => 1,
                        _ => 2,
                    }
                );

                // Set the after cursor for the next iteration
                cursor = page_info.end_cursor;
            }

            // Now use the last 'after' cursor as the 'before'
            for i in 0..3 {
                let components = client
                    .components_connection_query(None, cursor, None, Some(2))
                    .await
                    .unwrap()
                    .data
                    .unwrap()
                    .components;

                println!("api_graphql_components_connection 5-{}", i);

                // Total count should match the # of components
                assert_eq!(components.total_count, 5);

                let page_info = components.page_info;

                // Check prev/next paging. Since we're using a `before` cursor, the last
                // record won't be included, and therefore `has_next_page` will always be true.
                assert_eq!(
                    (page_info.has_previous_page, page_info.has_next_page),
                    match i {
                        0 => (true, true),
                        _ => (false, true),
                    }
                );

                // The # of `edges` results should be 2, and zero for the last iteration
                assert_eq!(
                    components.edges.iter().flatten().count(),
                    match i {
                        2 => 0,
                        _ => 2,
                    }
                );

                // Set the before cursor for the next iteration
                cursor = page_info.start_cursor;
            }

            println!("api_graphql_components_connection 6 END");
        })
        .await;
    }
}
