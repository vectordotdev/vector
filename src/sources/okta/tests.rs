use crate::components::validation::prelude::*;
use tokio::time::Duration;
use vector_lib::config::LogNamespace;
use warp::Filter;

use vector_lib::event::Event;

use crate::config::log_schema;
use crate::sources::okta::OktaConfig;
use crate::test_util::{
    components::HTTP_PULL_SOURCE_TAGS, components::run_and_assert_source_compliance, next_addr,
    test_generate_config, wait_for_tcp,
};

pub(crate) const INTERVAL: Duration = Duration::from_secs(10);

pub(crate) const TIMEOUT: Duration = Duration::from_secs(1);

/// The happy path should yield at least one event and must emit the required internal events for sources.
pub(crate) async fn run_compliance(config: OktaConfig) -> Vec<Event> {
    let events =
        run_and_assert_source_compliance(config, Duration::from_secs(5), &HTTP_PULL_SOURCE_TAGS)
            .await;

    assert!(!events.is_empty());

    events
}

#[test]
fn okta_generate_config() {
    test_generate_config::<OktaConfig>();
}

impl ValidatableComponent for OktaConfig {
    fn validation_configuration() -> ValidationConfiguration {
        let config = Self {
            domain: "foo.okta.com".to_string(),
            token: "token".to_string(),
            interval: Duration::from_secs(1),
            timeout: Duration::from_secs(1),
            ..Default::default()
        };
        let log_namespace: LogNamespace = config.log_namespace.unwrap_or_default().into();

        ValidationConfiguration::from_source(
            Self::NAME,
            log_namespace,
            vec![ComponentTestCaseConfig::from_source(config, None, None)],
        )
    }
}

register_validatable_component!(OktaConfig);

#[tokio::test]
async fn okta_compliance() {
    let in_addr = next_addr();

    let dummy_endpoint = warp::path!("api" / "v1" / "logs")
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .map(move |_| {
            warp::http::Response::builder()
                .header("Content-Type", "application/json")
                .body(r#"[{"data":"foo"},{"data":"bar"}]"#)
                .unwrap()
        });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    let events = run_compliance(OktaConfig {
        domain: format!("http://{in_addr}"),
        token: "token".to_string(),
        interval: INTERVAL,
        timeout: TIMEOUT,
        log_namespace: None,
        ..Default::default()
    })
    .await;

    assert_eq!(events.len(), 2);

    for event in events.iter() {
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            OktaConfig::NAME.into()
        );
    }
    let log_event = events[0].as_log();
    assert_eq!(
        log_event
            .get("data")
            .expect("data must be available")
            .as_str()
            .unwrap(),
        "foo"
    );
}

#[tokio::test]
async fn okta_follows_rel() {
    let addr = next_addr();

    let dummy_endpoint = warp::path!("api" / "v1" / "logs")
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .map({
            move |q: std::collections::HashMap<String, String>| match q.get("after") {
                None => warp::http::Response::builder()
                    .header("Content-Type", "application/json")
                    .header(
                        "link",
                        format!("<http://{addr}/api/v1/logs?after=bar>; rel=\"next\""),
                    )
                    .body(r#"[{"data":"foo"}]"#)
                    .unwrap(),
                Some(after) if after == "bar" => warp::http::Response::builder()
                    .header("Content-Type", "application/json")
                    .header(
                        "link",
                        format!("<http://{addr}/api/v1/logs?after=baz>; rel=\"next\""),
                    )
                    .body(r#"[{"data":"bar"}]"#)
                    .unwrap(),
                Some(after) if after == "baz" => warp::http::Response::builder()
                    .header("Content-Type", "application/json")
                    .header(
                        "link",
                        format!("<http://{addr}/api/v1/logs?after=quux>; rel=\"next\""),
                    )
                    .body(r#"[]"#)
                    .unwrap(),
                Some(_) => panic!("following Link header with zero length reply"),
            }
        });

    tokio::spawn(warp::serve(dummy_endpoint).run(addr));
    wait_for_tcp(addr).await;

    let events = run_compliance(OktaConfig {
        domain: format!("http://{addr}"),
        token: "token".to_string(),
        interval: INTERVAL,
        timeout: TIMEOUT,
        log_namespace: None,
        ..Default::default()
    })
    .await;

    assert_eq!(events.len(), 2);

    for event in events.iter() {
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            OktaConfig::NAME.into()
        );
    }
    assert_eq!(events[0].as_log()["data"].as_str().unwrap(), "foo");
    assert_eq!(events[1].as_log()["data"].as_str().unwrap(), "bar");
}

#[tokio::test]
async fn okta_persists_rel() {
    // the client follows `next` links; on the next interval it should pick up where it left off
    // and not start over from the beginning
    let addr = next_addr();
    let seen = tokio::sync::OnceCell::<bool>::new();

    // the first request sets `seen` but returns 0 events, ending the inner stream,
    // the next interval should pick up where it left off
    let dummy_endpoint = warp::path!("api" / "v1" / "logs")
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .map({
            move |q: std::collections::HashMap<String, String>| match q.get("after") {
                None => warp::http::Response::builder()
                    .header("Content-Type", "application/json")
                    .header(
                        "link",
                        format!("<http://{addr}/api/v1/logs?after=bar>; rel=\"next\""),
                    )
                    .body(r#"[{"data":"foo"}]"#)
                    .unwrap(),
                Some(after) if after == "bar" => {
                    if seen.initialized() {
                        warp::http::Response::builder()
                            .header("Content-Type", "application/json")
                            .header(
                                "link",
                                format!("<http://{addr}/api/v1/logs?after=baz>; rel=\"next\""),
                            )
                            .body(r#"[{"data":"bar"}]"#)
                            .unwrap()
                    } else {
                        seen.set(true).unwrap();
                        warp::http::Response::builder()
                            .header("Content-Type", "application/json")
                            .header(
                                "link",
                                format!("<http://{addr}/api/v1/logs?after=baz>; rel=\"next\""),
                            )
                            .body(r#"[]"#)
                            .unwrap()
                    }
                }
                Some(_) => warp::http::Response::builder()
                    .header("Content-Type", "application/json")
                    .body(r#"[]"#)
                    .unwrap(),
            }
        });

    tokio::spawn(warp::serve(dummy_endpoint).run(addr));
    wait_for_tcp(addr).await;

    let events = run_compliance(OktaConfig {
        domain: format!("http://{addr}"),
        token: "token".to_string(),
        interval: Duration::from_secs(1),
        timeout: Duration::from_millis(100),
        ..Default::default()
    })
    .await;

    assert_eq!(events.len(), 2);
}
