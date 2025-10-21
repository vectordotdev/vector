use std::collections::HashMap;

use http::{Response, StatusCode};
use tokio::time::Duration;
use vector_lib::{config::LogNamespace, event::Event};
use warp::Filter;

use crate::{
    components::validation::prelude::*,
    sources::okta::OktaConfig,
    test_util::{
        components::{
            COMPONENT_ERROR_TAGS, HTTP_PULL_SOURCE_TAGS, run_and_assert_source_compliance,
            run_and_assert_source_error,
        },
        next_addr, test_generate_config, wait_for_tcp,
    },
};

pub(crate) const INTERVAL: Duration = Duration::from_secs(10);

pub(crate) const TIMEOUT: Duration = Duration::from_secs(1);

/// Run queries against an Okta endpoint and verify compliance.
pub(crate) async fn run_compliance(config: OktaConfig) -> Vec<Event> {
    let events =
        run_and_assert_source_compliance(config, Duration::from_secs(3), &HTTP_PULL_SOURCE_TAGS)
            .await;

    assert!(!events.is_empty());

    events
}

/// The error path should not yield any events and must emit the required error internal events.
pub(crate) async fn run_error(config: OktaConfig) {
    let events =
        run_and_assert_source_error(config, Duration::from_secs(3), &COMPONENT_ERROR_TAGS).await;

    assert!(events.is_empty());
}

const OKTA_200_EMPTY: &str = r#"[]"#;
const OKTA_200_RESPONSE: &str = r#"
[
  {
    "actor": {
      "id": "00uttidj01jqL21aM1d6",
      "type": "User",
      "alternateId": "john.doe@example.com",
      "displayName": "John Doe",
      "detailEntry": null
    },
    "client": {
      "userAgent": {
        "rawUserAgent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36",
        "os": "Mac OS X",
        "browser": "CHROME"
      },
      "zone": null,
      "device": "Computer",
      "id": null,
      "ipAddress": "10.0.0.1",
      "geographicalContext": {
        "city": "New York",
        "state": "New York",
        "country": "United States",
        "postalCode": 10013,
        "geolocation": {
          "lat": 40.3157,
          "lon": -74.01
        }
      }
    },
    "device": {
      "id": "guofdhyjex1feOgbN1d9",
      "name": "Mac15,6",
      "os_platform": "OSX",
      "os_version": "14.6.0",
      "managed": false,
      "registered": true,
      "device_integrator": null,
      "disk_encryption_type": "ALL_INTERNAL_VOLUMES",
      "screen_lock_type": "BIOMETRIC",
      "jailbreak": null,
      "secure_hardware_present": true
    },
    "authenticationContext": {
      "authenticationProvider": null,
      "credentialProvider": null,
      "credentialType": null,
      "issuer": null,
      "interface": null,
      "authenticationStep": 0,
      "rootSessionId": "idxBager62CSveUkTxvgRtonA",
      "externalSessionId": "idxBager62CSveUkTxvgRtonA"
    },
    "displayMessage": "User login to Okta",
    "eventType": "user.session.start",
    "outcome": {
      "result": "SUCCESS",
      "reason": null
    },
    "published": "2024-08-13T15:58:20.353Z",
    "securityContext": {
      "asNumber": 394089,
      "asOrg": "ASN 0000",
      "isp": "google",
      "domain": null,
      "isProxy": false
    },
    "severity": "INFO",
    "debugContext": {
      "debugData": {
        "requestId": "ab609228fe84ce59cdcbfa690bcce016",
        "requestUri": "/idp/idx/authenticators/poll",
        "url": "/idp/idx/authenticators/poll"
      }
    },
    "legacyEventType": "core.user_auth.login_success",
    "transaction": {
      "type": "WEB",
      "id": "ab609228fe84ce59cdcbfa690bgce016",
      "detail": null
    },
    "uuid": "dc9fd3c0-598c-11ef-8478-2b7584bf8d5a",
    "version": 0,
    "request": {
      "ipChain": [
        {
          "ip": "10.0.0.1",
          "geographicalContext": {
            "city": "New York",
            "state": "New York",
            "country": "United States",
            "postalCode": 10013,
            "geolocation": {
              "lat": 40.3157,
              "lon": -74.01
            }
          },
          "version": "V4",
          "source": null
        }
      ]
    },
    "target": [
      {
        "id": "pfdfdhyjf0HMbkP2e1d7",
        "type": "AuthenticatorEnrollment",
        "alternateId": "unknown",
        "displayName": "Okta Verify",
        "detailEntry": null
      },
      {
        "id": "0oatxlef9sQvvqInq5d6",
        "type": "AppInstance",
        "alternateId": "Okta Admin Console",
        "displayName": "Okta Admin Console",
        "detailEntry": null
      }
    ]
  }
]
"#;

const OKTA_400_VALIDATION_FAILED: &str = r#"
{
  "errorCode": "E0000001",
  "errorSummary": "Api validation failed: {0}",
  "errorLink": "E0000001",
  "errorId": "sampleiCF-8D5rLW6myqiPItW",
  "errorCauses": []
}
"#;

const OKTA_403_ACCESS_DENIED: &str = r#"
{
  "errorCode": "E0000006",
  "errorSummary": "You do not have permission to perform the requested action",
  "errorLink": "E0000006",
  "errorId": "sampleNUSD_8fdkFd8fs8SDBK",
  "errorCauses": []
}
"#;

const OKTA_429_RATE_LIMIT_EXCEEDED: &str = r#"
{
  "errorCode": "E0000047",
  "errorSummary": "API call exceeded rate limit due to too many requests.",
  "errorLink": "E0000047",
  "errorId": "sampleQPivGUj_ND5v78vbYWW",
  "errorCauses": []
}
"#;

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
async fn with_default_config() {
    let in_addr = next_addr();

    let dummy_endpoint = warp::path!("api" / "v1" / "logs")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(warp::header::exact("Accept", "application/json"))
        .map(|params: HashMap<String, String>| {
            assert!(params.contains_key("since"));
            Response::builder()
                .status(StatusCode::OK)
                .body(OKTA_200_EMPTY)
                .unwrap()
        });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    run_compliance(OktaConfig {
        domain: format!("http://{in_addr}"),
        token: "token".to_string(),
        interval: INTERVAL,
        timeout: TIMEOUT,
        ..Default::default()
    })
    .await;
}

#[tokio::test]
async fn with_okta_events() {
    let in_addr = next_addr();

    let dummy_endpoint = warp::path!("api" / "v1" / "logs")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(warp::header::exact("Accept", "application/json"))
        .map(|params: HashMap<String, String>| {
            assert!(params.contains_key("since"));
            Response::builder()
                .status(StatusCode::OK)
                .body(OKTA_200_RESPONSE)
                .unwrap()
        });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    run_compliance(OktaConfig {
        domain: format!("http://{in_addr}"),
        token: "token".to_string(),
        interval: INTERVAL,
        timeout: TIMEOUT,
        ..Default::default()
    })
    .await;
}

#[tokio::test]
async fn with_bad_request() {
    let in_addr = next_addr();

    let dummy_endpoint = warp::path!("api" / "v1" / "logs")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(warp::header::exact("Accept", "application/json"))
        .map(|_| {
            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(OKTA_400_VALIDATION_FAILED)
                .unwrap()
        });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    run_error(OktaConfig {
        domain: format!("http://{in_addr}"),
        token: "token".to_string(),
        interval: INTERVAL,
        timeout: TIMEOUT,
        ..Default::default()
    })
    .await;
}

#[tokio::test]
async fn with_bad_token() {
    let in_addr = next_addr();

    let dummy_endpoint = warp::path!("api" / "v1" / "logs")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(warp::header::exact("Accept", "application/json"))
        .map(|_| {
            Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(OKTA_403_ACCESS_DENIED)
                .unwrap()
        });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    run_error(OktaConfig {
        domain: format!("http://{in_addr}"),
        token: "badtoken".to_string(),
        interval: INTERVAL,
        timeout: TIMEOUT,
        ..Default::default()
    })
    .await;
}

#[tokio::test]
async fn with_rate_limit_exceeded() {
    let in_addr = next_addr();

    let dummy_endpoint = warp::path!("api" / "v1" / "logs")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(warp::header::exact("Accept", "application/json"))
        .map(|_| {
            Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .body(OKTA_429_RATE_LIMIT_EXCEEDED)
                .unwrap()
        });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    run_error(OktaConfig {
        domain: format!("http://{in_addr}"),
        token: "token".to_string(),
        interval: INTERVAL,
        timeout: TIMEOUT,
        ..Default::default()
    })
    .await;
}
