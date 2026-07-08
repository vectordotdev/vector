use snafu::Snafu;
use vector_lib::configurable::configurable_component;

mod compression;
mod config;
mod service;
mod sink;

pub use config::VectorConfig;

/// Marker type for the version two of the configuration for the `vector` sink.
#[configurable_component]
#[derive(Clone, Debug)]
enum VectorConfigVersion {
    /// Marker value for version two.
    #[serde(rename = "2")]
    V2,
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum VectorSinkError {
    #[snafu(display("Request failed: {}", source))]
    Request { source: tonic::Status },

    #[snafu(display("Vector source unhealthy: {:?}", status))]
    Health { status: Option<&'static str> },

    #[snafu(display("URL has no host."))]
    NoHost,
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::VectorConfig>();
    }
}

#[cfg(test)]
mod tests {
    use bytes::{BufMut, Bytes, BytesMut};
    use futures::{StreamExt, channel::mpsc};
    use http::request::Parts;
    use hyper::{
        Method, Response, Server,
        service::{make_service_fn, service_fn},
    };
    use prost::Message;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use vector_lib::{
        config::{Tags, Telemetry, init_telemetry},
        event::{BatchNotifier, BatchStatus},
    };

    use super::{config::with_default_scheme, *};
    use crate::{
        config::{SinkConfig as _, SinkContext},
        event::Event,
        proto::vector as proto,
        sinks::util::test::build_test_server_generic,
        test_util::{
            addr::next_addr,
            components::{
                DATA_VOLUME_SINK_TAGS, HTTP_SINK_TAGS, run_and_assert_data_volume_sink_compliance,
                run_and_assert_sink_compliance,
            },
            random_lines_with_stream,
        },
    };

    // one byte for the compression flag plus four bytes for the length
    const GRPC_HEADER_SIZE: usize = 5;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<VectorConfig>();
    }

    #[tokio::test]
    async fn build_rejects_missing_address() {
        let config: VectorConfig = toml::from_str("").unwrap();

        let err = match config.build(SinkContext::default()).await {
            Ok(_) => panic!("missing address should fail"),
            Err(err) => err,
        };

        assert!(
            err.to_string().contains(
                "No Vector endpoint configured. Please set `address` or `routing.endpoints`."
            ),
            "{err}"
        );
    }

    #[tokio::test]
    async fn build_rejects_address_and_routing() {
        let config: VectorConfig = toml::from_str(
            r#"
                address = "http://127.0.0.1:6000"

                [routing]
                endpoints = ["http://127.0.0.1:6001"]
            "#,
        )
        .unwrap();

        let err = match config.build(SinkContext::default()).await {
            Ok(_) => panic!("address and routing should be mutually exclusive"),
            Err(err) => err,
        };

        assert!(
            err.to_string()
                .contains("`address` and `routing` options are mutually exclusive"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn build_rejects_empty_routing_endpoints() {
        let config: VectorConfig = toml::from_str(
            r#"
                [routing]
                strategy = "failover"
            "#,
        )
        .unwrap();

        let err = match config.build(SinkContext::default()).await {
            Ok(_) => panic!("routing without endpoints should fail"),
            Err(err) => err,
        };

        assert!(
            err.to_string()
                .contains("`routing.endpoints` must contain at least one endpoint"),
            "{err}"
        );
    }

    #[test]
    fn parse_routing_config() {
        let config: Result<VectorConfig, _> = toml::from_str(
            r#"
                [routing]
                endpoints = ["http://127.0.0.1:6000", "http://127.0.0.1:6001"]
            "#,
        );

        assert!(config.is_ok());
    }

    #[test]
    fn parse_failover_endpoint_strategy() {
        let config: Result<VectorConfig, _> = toml::from_str(
            r#"
                [routing]
                endpoints = ["http://127.0.0.1:6000", "http://127.0.0.1:6001"]
                strategy = "failover"
            "#,
        );

        assert!(config.is_ok());
    }

    #[test]
    fn parse_failover_primary_endpoint_strategy() {
        let config: Result<VectorConfig, _> = toml::from_str(
            r#"
                [routing]
                endpoints = ["http://127.0.0.1:6000", "http://127.0.0.1:6001"]
                strategy = "failover_primary"
            "#,
        );

        assert!(config.is_ok());
    }

    enum TestType {
        Normal,
        DataVolume,
    }

    async fn run_sink_test(test_type: TestType) {
        run_sink_test_with_compression(test_type, None).await;
    }

    async fn run_sink_test_with_compression(test_type: TestType, compression: Option<&str>) {
        let num_lines = 10;

        let (_guard, in_addr) = next_addr();

        let config = match compression {
            Some(c) => indoc::formatdoc! {r#"
                address: "http://{in_addr}/"
                compression: "{c}"
            "#},
            None => format!("address: \"http://{in_addr}/\"\n"),
        };
        let config: VectorConfig = serde_yaml::from_str(&config).unwrap();

        let cx = SinkContext::default();

        let (sink, _) = config.build(cx).await.unwrap();
        let (rx, trigger, server) = build_test_server_generic(in_addr, move || {
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });

        tokio::spawn(server);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        match test_type {
            TestType::Normal => run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await,

            TestType::DataVolume => {
                run_and_assert_data_volume_sink_compliance(sink, events, &DATA_VOLUME_SINK_TAGS)
                    .await
            }
        }

        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let expected_encoding = compression;
        let output_lines = get_received(rx, move |parts| {
            assert_eq!(Method::POST, parts.method);
            assert_eq!("/vector.Vector/PushEvents", parts.uri.path());
            assert_eq!(
                "application/grpc",
                parts.headers.get("content-type").unwrap().to_str().unwrap()
            );
            match expected_encoding {
                Some(enc) => assert_eq!(
                    enc,
                    parts
                        .headers
                        .get("grpc-encoding")
                        .unwrap_or_else(|| panic!("missing grpc-encoding header (expected {enc})"))
                        .to_str()
                        .unwrap()
                ),
                None => assert!(
                    parts.headers.get("grpc-encoding").is_none(),
                    "unexpected grpc-encoding header present"
                ),
            }
        })
        .await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[tokio::test]
    async fn deliver_message() {
        run_sink_test(TestType::Normal).await;
    }

    #[tokio::test]
    async fn deliver_message_to_multiple_endpoints() {
        let num_lines = 10;

        let (_guard1, addr1) = next_addr();
        let (_guard2, addr2) = next_addr();

        let config = format!(
            r#"
                [routing]
                endpoints = ["http://{addr1}/", "http://{addr2}/"]
            "#
        );
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let cx = SinkContext::default();

        let (sink, _) = config.build(cx).await.unwrap();
        let (rx1, trigger1, server1) = build_test_server_generic(addr1, move || {
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });
        let (rx2, trigger2, server2) = build_test_server_generic(addr2, move || {
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });

        tokio::spawn(server1);
        tokio::spawn(server2);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (mut input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

        drop(trigger1);
        drop(trigger2);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let (mut output_lines, mut output_lines2) =
            futures::future::join(get_received(rx1, |_| {}), get_received(rx2, |_| {})).await;

        output_lines.append(&mut output_lines2);

        input_lines.sort();
        output_lines.sort();

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[tokio::test]
    async fn failover_strategy_prefers_first_endpoint() {
        let num_lines = 10;

        let (_guard1, addr1) = next_addr();
        let (_guard2, addr2) = next_addr();

        let config = format!(
            r#"
                [routing]
                endpoints = ["http://{addr1}/", "http://{addr2}/"]
                strategy = "failover"
            "#
        );
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let (sink, _) = config.build(SinkContext::default()).await.unwrap();
        let (rx1, trigger1, server1) = build_test_server_generic(addr1, move || {
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });
        let (rx2, trigger2, server2) = build_test_server_generic(addr2, move || {
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });

        tokio::spawn(server1);
        tokio::spawn(server2);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

        drop(trigger1);
        drop(trigger2);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let (output_lines, output_lines2) =
            futures::future::join(get_received(rx1, |_| {}), get_received(rx2, |_| {})).await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
        assert!(output_lines2.is_empty());
    }

    #[tokio::test]
    async fn failover_strategy_uses_next_endpoint_when_first_fails() {
        let num_lines = 10;

        let (_guard1, addr1) = next_addr();
        let (_guard2, addr2) = next_addr();

        let config = format!(
            r#"
                [routing]
                endpoints = ["http://{addr1}/", "http://{addr2}/"]
                strategy = "failover"
            "#
        );
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let (sink, _) = config.build(SinkContext::default()).await.unwrap();
        let (rx2, trigger2, server2) = build_test_server_generic(addr2, move || {
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });

        tokio::spawn(server2);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

        drop(trigger2);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let output_lines = get_received(rx2, |_| {}).await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[tokio::test]
    async fn failover_primary_strategy_retries_primary_before_secondary() {
        let num_lines = 10;

        let (_guard1, addr1) = next_addr();
        let (_guard2, addr2) = next_addr();

        let config = format!(
            r#"
                [routing]
                endpoints = ["http://{addr1}/", "http://{addr2}/"]
                strategy = "failover_primary"
            "#
        );
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let primary_attempts = Arc::new(AtomicUsize::new(0));
        let primary_service_attempts = Arc::clone(&primary_attempts);
        let (rx1, trigger1, server1) = build_test_server_generic(addr1, move || {
            if primary_service_attempts.fetch_add(1, Ordering::AcqRel) == 0 {
                hyper::Response::builder()
                    .header("grpc-status", "14") // unavailable
                    .header("content-type", "application/grpc")
                    .body(hyper::Body::empty())
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .header("grpc-status", "0") // OK
                    .header("content-type", "application/grpc")
                    .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                    .unwrap()
            }
        });
        let (rx2, trigger2, server2) = build_test_server_generic(addr2, move || {
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });

        tokio::spawn(server1);
        tokio::spawn(server2);

        let (sink, _) = config.build(SinkContext::default()).await.unwrap();

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

        drop(trigger1);
        drop(trigger2);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
        assert_eq!(
            primary_attempts.load(Ordering::Acquire),
            2,
            "retriable primary failure should retry configured primary before secondary"
        );

        let output_lines = get_received(rx1, |_| {}).await;
        let secondary_output_lines = get_received(rx2, |_| {}).await;

        assert_eq!(num_lines * 2, output_lines.len());
        assert!(
            output_lines
                .chunks(num_lines)
                .all(|lines| lines == input_lines)
        );
        assert!(
            secondary_output_lines.is_empty(),
            "secondary must not receive traffic when configured primary succeeds on retry"
        );
    }

    #[tokio::test]
    async fn failover_strategy_continues_in_ring_order_after_active_failure() {
        let num_lines = 2;

        let (_guard1, addr1) = next_addr();
        let (_guard2, addr2) = next_addr();
        let (_guard3, addr3) = next_addr();

        let config = format!(
            r#"
                batch.max_events = 1
                request.concurrency = 1

                [routing]
                endpoints = ["http://{addr1}/", "http://{addr2}/", "http://{addr3}/"]
                strategy = "failover"
            "#
        );
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let primary_attempts = Arc::new(AtomicUsize::new(0));
        let primary_service_attempts = Arc::clone(&primary_attempts);
        let (_rx1, trigger1, server1) = build_test_server_generic(addr1, move || {
            primary_service_attempts.fetch_add(1, Ordering::AcqRel);
            hyper::Response::builder()
                .header("grpc-status", "14") // unavailable
                .header("content-type", "application/grpc")
                .body(hyper::Body::empty())
                .unwrap()
        });

        let active_attempts = Arc::new(AtomicUsize::new(0));
        let active_service_attempts = Arc::clone(&active_attempts);
        let (_rx2, trigger2, server2) = build_test_server_generic(addr2, move || {
            if active_service_attempts.fetch_add(1, Ordering::AcqRel) == 0 {
                hyper::Response::builder()
                    .header("grpc-status", "0") // OK
                    .header("content-type", "application/grpc")
                    .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .header("grpc-status", "14") // unavailable
                    .header("content-type", "application/grpc")
                    .body(hyper::Body::empty())
                    .unwrap()
            }
        });

        let next_attempts = Arc::new(AtomicUsize::new(0));
        let next_service_attempts = Arc::clone(&next_attempts);
        let (_rx3, trigger3, server3) = build_test_server_generic(addr3, move || {
            next_service_attempts.fetch_add(1, Ordering::AcqRel);
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });

        tokio::spawn(server1);
        tokio::spawn(server2);
        tokio::spawn(server3);

        let (sink, _) = config.build(SinkContext::default()).await.unwrap();
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

        drop(trigger1);
        drop(trigger2);
        drop(trigger3);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
        assert_eq!(primary_attempts.load(Ordering::Acquire), 1);
        assert_eq!(active_attempts.load(Ordering::Acquire), 2);
        assert_eq!(next_attempts.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn failover_primary_strategy_retries_configured_primary_after_active_failure() {
        let num_lines = 2;

        let (_guard1, addr1) = next_addr();
        let (_guard2, addr2) = next_addr();
        let (_guard3, addr3) = next_addr();

        let config = format!(
            r#"
                batch.max_events = 1
                request.concurrency = 1

                [routing]
                endpoints = ["http://{addr1}/", "http://{addr2}/", "http://{addr3}/"]
                strategy = "failover_primary"
            "#
        );
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let primary_attempts = Arc::new(AtomicUsize::new(0));
        let primary_service_attempts = Arc::clone(&primary_attempts);
        let (_rx1, trigger1, server1) = build_test_server_generic(addr1, move || {
            if primary_service_attempts.fetch_add(1, Ordering::AcqRel) < 2 {
                hyper::Response::builder()
                    .header("grpc-status", "14") // unavailable
                    .header("content-type", "application/grpc")
                    .body(hyper::Body::empty())
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .header("grpc-status", "0") // OK
                    .header("content-type", "application/grpc")
                    .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                    .unwrap()
            }
        });

        let active_attempts = Arc::new(AtomicUsize::new(0));
        let active_service_attempts = Arc::clone(&active_attempts);
        let (_rx2, trigger2, server2) = build_test_server_generic(addr2, move || {
            if active_service_attempts.fetch_add(1, Ordering::AcqRel) == 0 {
                hyper::Response::builder()
                    .header("grpc-status", "0") // OK
                    .header("content-type", "application/grpc")
                    .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                    .unwrap()
            } else {
                hyper::Response::builder()
                    .header("grpc-status", "14") // unavailable
                    .header("content-type", "application/grpc")
                    .body(hyper::Body::empty())
                    .unwrap()
            }
        });

        let next_attempts = Arc::new(AtomicUsize::new(0));
        let next_service_attempts = Arc::clone(&next_attempts);
        let (_rx3, trigger3, server3) = build_test_server_generic(addr3, move || {
            next_service_attempts.fetch_add(1, Ordering::AcqRel);
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });

        tokio::spawn(server1);
        tokio::spawn(server2);
        tokio::spawn(server3);

        let (sink, _) = config.build(SinkContext::default()).await.unwrap();
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

        drop(trigger1);
        drop(trigger2);
        drop(trigger3);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
        assert_eq!(primary_attempts.load(Ordering::Acquire), 3);
        assert_eq!(active_attempts.load(Ordering::Acquire), 2);
        assert_eq!(next_attempts.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn failover_strategy_does_not_resend_non_retriable_errors() {
        let num_lines = 10;

        let (_guard1, addr1) = next_addr();
        let (_guard2, addr2) = next_addr();

        let config = format!(
            r#"
                [routing]
                endpoints = ["http://{addr1}/", "http://{addr2}/"]
                strategy = "failover"
            "#
        );
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let (sink, _) = config.build(SinkContext::default()).await.unwrap();
        let (_rx1, trigger1, server1) = build_test_server_generic(addr1, move || {
            hyper::Response::builder()
                .header("grpc-status", "15") // data loss
                .header("content-type", "application/grpc")
                .body(tonic::body::empty_body())
                .unwrap()
        });
        let (rx2, trigger2, server2) = build_test_server_generic(addr2, move || {
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });

        tokio::spawn(server1);
        tokio::spawn(server2);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_, events) = random_lines_with_stream(8, num_lines, Some(batch));

        sink.run(events).await.expect("Running sink failed");

        drop(trigger1);
        drop(trigger2);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
        assert!(
            get_received(rx2, |_| {}).await.is_empty(),
            "non-retriable primary rejection must not be resent to secondary endpoint"
        );
    }

    #[tokio::test]
    async fn failover_strategy_uses_next_endpoint_when_first_times_out() {
        let num_lines = 10;

        let (_guard1, addr1) = next_addr();
        let (_guard2, addr2) = next_addr();

        let config = format!(
            r#"
                [request]
                timeout_secs = 1

                [routing]
                endpoints = ["http://{addr1}/", "http://{addr2}/"]
                strategy = "failover"
            "#
        );
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let hanging_service = make_service_fn(|_| async {
            Ok::<_, crate::Error>(service_fn(|_req| async {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                Ok::<_, crate::Error>(
                    Response::builder()
                        .header("grpc-status", "0") // OK
                        .header("content-type", "application/grpc")
                        .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                        .unwrap(),
                )
            }))
        });
        let hanging_server = tokio::spawn(Server::bind(&addr1).serve(hanging_service));

        let (sink, _) = config.build(SinkContext::default()).await.unwrap();
        let (rx2, trigger2, server2) = build_test_server_generic(addr2, move || {
            hyper::Response::builder()
                .header("grpc-status", "0") // OK
                .header("content-type", "application/grpc")
                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                .unwrap()
        });

        tokio::spawn(server2);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

        hanging_server.abort();
        drop(trigger2);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let output_lines = get_received(rx2, |_| {}).await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[tokio::test]
    async fn failover_strategy_retries_after_all_endpoints_time_out() {
        let num_lines = 10;

        let (_guard, addr) = next_addr();

        let config = format!(
            r#"
                [request]
                timeout_secs = 1
                retry_initial_backoff_secs = 1
                retry_max_duration_secs = 5

                [routing]
                endpoints = ["http://{addr}/"]
                strategy = "failover"
            "#
        );
        let config: VectorConfig = toml::from_str(&config).unwrap();

        let attempts = Arc::new(AtomicUsize::new(0));
        let service_attempts = Arc::clone(&attempts);
        let service = make_service_fn(move |_| {
            let service_attempts = Arc::clone(&service_attempts);
            async move {
                Ok::<_, crate::Error>(service_fn(move |_req| {
                    let attempt = service_attempts.fetch_add(1, Ordering::AcqRel);
                    async move {
                        if attempt == 0 {
                            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                        }

                        Ok::<_, crate::Error>(
                            Response::builder()
                                .header("grpc-status", "0") // OK
                                .header("content-type", "application/grpc")
                                .body(hyper::Body::from(encode_body(proto::PushEventsResponse {})))
                                .unwrap(),
                        )
                    }
                }))
            }
        });
        let server = tokio::spawn(Server::bind(&addr).serve(service));

        let (sink, _) = config.build(SinkContext::default()).await.unwrap();

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_input_lines, events) = random_lines_with_stream(8, num_lines, Some(batch));

        run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

        server.abort();

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
        assert!(
            attempts.load(Ordering::Acquire) > 1,
            "sink should retry after endpoint timeout"
        );
    }

    #[tokio::test]
    async fn deliver_message_gzip() {
        run_sink_test_with_compression(TestType::Normal, Some("gzip")).await;
    }

    #[tokio::test]
    async fn deliver_message_zstd() {
        run_sink_test_with_compression(TestType::Normal, Some("zstd")).await;
    }

    #[tokio::test]
    async fn deliver_message_none() {
        run_sink_test_with_compression(TestType::Normal, None).await;
    }

    #[tokio::test]
    async fn data_volume_tags() {
        init_telemetry(
            Telemetry {
                tags: Tags {
                    emit_service: true,
                    emit_source: true,
                },
            },
            true,
        );

        run_sink_test(TestType::DataVolume).await;
    }

    #[tokio::test]
    async fn acknowledges_error() {
        let num_lines = 10;

        let (_guard, in_addr) = next_addr();

        let config = format!("address: \"http://{in_addr}/\"");
        let config: VectorConfig = serde_yaml::from_str(&config).unwrap();

        let cx = SinkContext::default();

        let (sink, _) = config.build(cx).await.unwrap();
        let (_rx, trigger, server) = build_test_server_generic(in_addr, move || {
            hyper::Response::builder()
                .header("grpc-status", "7") // permission denied
                .header("content-type", "application/grpc")
                .body(tonic::body::empty_body())
                .unwrap()
        });

        tokio::spawn(server);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_, events) = random_lines_with_stream(8, num_lines, Some(batch));

        sink.run(events).await.expect("Running sink failed");

        drop(trigger);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
    }

    #[test]
    fn test_with_default_scheme() {
        assert_eq!(
            with_default_scheme("0.0.0.0", false).unwrap().to_string(),
            "http://0.0.0.0/"
        );
        assert_eq!(
            with_default_scheme("0.0.0.0", true).unwrap().to_string(),
            "https://0.0.0.0/"
        );
    }

    async fn get_received(
        rx: mpsc::Receiver<(Parts, Bytes)>,
        assert_parts: impl Fn(Parts),
    ) -> Vec<String> {
        rx.map(|(parts, body)| {
            let encoding = parts
                .headers
                .get("grpc-encoding")
                .map(|v| v.to_str().unwrap().to_owned());
            assert_parts(parts);

            let compressed = body[0] == 1;
            let proto_body = body.slice(GRPC_HEADER_SIZE..);
            let proto_body = if compressed {
                use std::io::Read;
                let mut out = Vec::new();
                match encoding.as_deref() {
                    Some("gzip") => {
                        flate2::read::GzDecoder::new(&proto_body[..])
                            .read_to_end(&mut out)
                            .unwrap();
                    }
                    Some("zstd") => {
                        zstd::stream::read::Decoder::new(&proto_body[..])
                            .unwrap()
                            .read_to_end(&mut out)
                            .unwrap();
                    }
                    other => panic!("unexpected grpc-encoding for compressed frame: {other:?}"),
                }
                Bytes::from(out)
            } else {
                proto_body
            };

            let req = proto::PushEventsRequest::decode(proto_body).unwrap();

            let mut events = Vec::with_capacity(req.events.len());
            for event in req.events {
                let event: Event = event.into();
                let string = event
                    .as_log()
                    .get("message")
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                events.push(string)
            }

            events
        })
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect()
    }

    // taken from <https://github.com/hyperium/tonic/blob/5aa8ae1fec27377cd4c2a41d309945d7e38087d0/examples/src/grpc-web/client.rs#L45-L75>
    fn encode_body<T>(msg: T) -> Bytes
    where
        T: prost::Message,
    {
        let mut buf = BytesMut::with_capacity(1024);

        // first skip past the header
        // cannot write it yet since we don't know the size of the
        // encoded message
        buf.reserve(GRPC_HEADER_SIZE);
        unsafe {
            buf.advance_mut(GRPC_HEADER_SIZE);
        }

        // write the message
        msg.encode(&mut buf).unwrap();

        // now we know the size of encoded message and can write the
        // header
        let len = buf.len() - GRPC_HEADER_SIZE;
        {
            let mut buf = &mut buf[..GRPC_HEADER_SIZE];

            // compression flag, 0 means "no compression"
            buf.put_u8(0);

            buf.put_u32(len as u32);
        }

        buf.split_to(len + GRPC_HEADER_SIZE).freeze()
    }
}
