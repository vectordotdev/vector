#[cfg(feature = "api")]
#[macro_use]
extern crate matches;

mod support;

#[cfg(all(feature = "api", feature = "vector-api-client"))]
mod tests {
    use crate::support::{fork_test, sink, source_with_event_counter, transform};
    use chrono::Utc;
    use futures::StreamExt;
    use metrics::counter;
    use std::{
        collections::HashMap,
        net::SocketAddr,
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

    // Initialize the metrics system.
    fn init_metrics() -> oneshot::Sender<()> {
        vector::trace::init(true, true, "info");
        let _ = vector::metrics::init();

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

    /// Invokes `fork_test`, and initializes metrics
    fn metrics_test<T: std::future::Future>(test_name: &'static str, fut: T) {
        fork_test(test_name, async move {
            let _metrics = init_metrics();
            fut.await;
        })
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

    #[test]
    /// Tests links between components
    fn api_graphql_component_links() {
        fork_test("tests::api_graphql_component_links", async {
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

            let res = client
                .component_links_query(None, None, None, None)
                .await
                .unwrap();

            let data = res.data.unwrap();
            let sources = data
                .sources
                .edges
                .into_iter()
                .flatten()
                .filter_map(std::convert::identity)
                .collect::<Vec<_>>();

            let transforms = data
                .transforms
                .edges
                .into_iter()
                .flatten()
                .filter_map(std::convert::identity)
                .collect::<Vec<_>>();

            let sinks = data
                .sinks
                .edges
                .into_iter()
                .flatten()
                .filter_map(std::convert::identity)
                .collect::<Vec<_>>();

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
        })
    }

    #[tokio::test]
    /// tests that version_string meta matches the current Vector version
    async fn api_graphql_meta_version_string() {
        let server = start_server();
        let client = make_client(server.addr());

        let res = client.meta_version_string().await.unwrap();

        assert_eq!(res.data.unwrap().meta.version_string, vector::get_version());
    }

    #[test]
    /// Tests that the heartbeat subscription returns a UTC payload every 1/2 second
    fn api_graphql_heartbeat() {
        metrics_test("tests::api_graphql_heartbeat", async {
            let server = start_server();
            let client = new_subscription_client(server.addr()).await;

            new_heartbeat_subscription(&client, 3, 500).await;
        })
    }

    #[test]
    /// Tests for Vector instance uptime in seconds
    fn api_graphql_uptime_metrics() {
        metrics_test("tests::api_graphql_uptime_metrics", async {
            let server = start_server();
            let client = new_subscription_client(server.addr()).await;

            new_uptime_subscription(&client).await;
        })
    }

    #[test]
    /// Tests for events processed metrics, using fake generator events
    fn api_graphql_event_processed_total_metrics() {
        metrics_test("tests::api_graphql_event_processed_total_metrics", async {
            let server = start_server();
            let client = new_subscription_client(server.addr()).await;

            new_processed_events_total_subscription(&client, 3, 100).await;
        })
    }

    #[test]
    /// Tests whether 2 disparate subscriptions can run against a single client
    fn api_graphql_combined_heartbeat_uptime() {
        metrics_test("tests::api_graphql_combined_heartbeat_uptime", async {
            let server = start_server();
            let client = new_subscription_client(server.addr()).await;

            futures::join! {
                new_uptime_subscription(&client),
                new_heartbeat_subscription(&client, 3, 500),
            };
        })
    }

    #[test]
    #[allow(clippy::float_cmp)]
    /// Tests componentProcessedEventsTotals returns increasing metrics, ordered by
    /// source -> transform -> sink
    fn api_graphql_component_processed_events_totals() {
        metrics_test(
            "tests::api_graphql_component_processed_events_totals",
            async {
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

                tokio::time::delay_for(tokio::time::Duration::from_millis(500)).await;

                let server = api::Server::start(topology.config());
                let client = new_subscription_client(server.addr()).await;
                let subscription = client.component_processed_events_totals_subscription(500);

                let data = subscription
                    .stream()
                    .skip(1)
                    .take(1)
                    .map(|r| r.unwrap().data.unwrap().component_processed_events_totals)
                    .next()
                    .await
                    .expect("Didn't return results");

                for name in &[
                    "processed_events_total_batch_source",
                    "processed_events_total_batch_sink",
                ] {
                    assert!(data.iter().any(|d| d.name == *name));
                }
            },
        )
    }

    #[test]
    #[allow(clippy::float_cmp)]
    /// Tests componentProcessedBytesTotals returns increasing metrics, ordered by
    /// source -> transform -> sink
    fn api_graphql_component_processed_bytes_totals() {
        metrics_test(
            "tests::api_graphql_component_processed_bytes_totals",
            async {
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

                let server = api::Server::start(topology.config());
                let client = new_subscription_client(server.addr()).await;
                let subscription = client.component_processed_bytes_totals_subscription(500);

                let data = subscription
                    .stream()
                    .skip(1)
                    .take(1)
                    .map(|r| r.unwrap().data.unwrap().component_processed_bytes_totals)
                    .next()
                    .await
                    .expect("Didn't return results");

                // Bytes are currently only relevant on sinks
                assert_eq!(data[0].name, "processed_bytes_total_batch_sink");
                assert!(data[0].metric.processed_bytes_total > 0.00);
            },
        )
    }

    #[test]
    /// Tests componentAdded receives an added component
    fn api_graphql_component_added_subscription() {
        metrics_test("tests::api_graphql_component_added_subscription", async {
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

            topology.reload_config_and_respawn(c).await.unwrap();
            server.update_config(topology.config());

            // Await the join handle
            handle.await.unwrap();
        })
    }

    #[test]
    /// Tests componentRemoves detects when a component has been removed
    fn api_graphql_component_removed_subscription() {
        metrics_test("tests::api_graphql_component_removed_subscription", async {
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

            topology.reload_config_and_respawn(c).await.unwrap();
            server.update_config(topology.config());

            // Await the join handle
            handle.await.unwrap();
        })
    }

    #[test]
    fn api_graphql_errors_total() {
        metrics_test("tests::api_graphql_errors_total", async {
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

            let server = api::Server::start(topology.config());
            let client = new_subscription_client(server.addr()).await;

            // Spawn a handler for listening to changes
            let handle = tokio::spawn(async move {
                let subscription = client.errors_total_subscription(50);

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
            });

            // Emit an error metric
            counter!("processing_errors_total", 1);

            handle.await.unwrap()
        });
    }

    #[test]
    fn api_grahql_component_errors_total() {
        metrics_test("tests::api_grahql_component_errors_total", async {
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

            let server = api::Server::start(topology.config());
            let client = new_subscription_client(server.addr()).await;

            // Spawn a handler for listening to changes
            let handle = tokio::spawn(async move {
                let subscription = client.errors_total_subscription(50);

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
            });

            // Emit an error metric
            counter!("processing_errors_total", 1);

            handle.await.unwrap()
        });
    }

    #[cfg(unix)]
    #[test]
    fn api_graphql_files_source_metrics() {
        use std::io::Write;
        use tempfile::{tempdir, NamedTempFile};

        metrics_test("tests::api_graphql_files_source_metrics", async {
            let lines = vec!["test1", "test2", "test3"];

            let checkpoints = tempdir().unwrap();
            let mut named_file = NamedTempFile::new().unwrap();
            let path = named_file.path().to_str().unwrap().to_string();
            let mut file = named_file.as_file_mut();

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
            let server = api::Server::start(topology.config());

            // Short delay to ensure logs are picked up
            tokio::time::delay_for(tokio::time::Duration::from_millis(200)).await;

            let client = make_client(server.addr());
            let res = client
                .file_source_metrics_query(None, None, None, None)
                .await;

            match &res.unwrap().data.unwrap().sources.edges.into_iter().flatten().next().unwrap().unwrap().node.metrics.on {
                file_source_metrics_query::FileSourceMetricsQuerySourcesEdgesNodeMetricsOn::FileSourceMetrics(
                    file_source_metrics_query::FileSourceMetricsQuerySourcesEdgesNodeMetricsOnFileSourceMetrics { files, .. },
                ) => {
                    let node = &files.edges.iter().flatten().next().unwrap().as_ref().unwrap().node;
                    assert_eq!(node.name, path);
                    assert_eq!(node.processed_events_total.as_ref().unwrap().processed_events_total as usize, lines.len());
                }
                _ => panic!("not a file source"),
            }
        })
    }

    #[test]
    fn api_graphql_component_by_name() {
        metrics_test("tests::api_graphql_component_by_name", async {
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
            let server = api::Server::start(topology.config());
            let client = make_client(server.addr());

            // Retrieving a component that doesn't exist should return None
            let res = client.component_by_name_query("xxx").await;
            assert!(res.unwrap().data.unwrap().component_by_name.is_none());

            // The `gen1` name should exist
            let res = client.component_by_name_query("gen1").await;
            assert_eq!(
                res.unwrap().data.unwrap().component_by_name.unwrap().name,
                "gen1"
            );
        })
    }

    #[test]
    fn api_graphql_components_connection() {
        metrics_test("tests::api_graphql_components_connection", async {
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

            let server = api::Server::start(topology.config());
            let client = make_client(server.addr());

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
        });
    }
}
