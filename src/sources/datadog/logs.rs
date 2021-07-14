use crate::sources::util::{ErrorMessage, HttpSource, HttpSourceAuthConfig};
use crate::{
    config::{
        log_schema, DataType, GenerateConfig, Resource, SourceConfig, SourceContext,
        SourceDescription,
    },
    event::Event,
    sources,
    tls::TlsConfig,
};
use bytes::Bytes;
use chrono::Utc;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use vector_core::event::LogEvent;
use warp::http::StatusCode;

use warp::http::HeaderMap;

lazy_static! {
    static ref API_KEY_MATCHER: Regex =
        Regex::new(r"^/v1/input/(?P<api_key>[[:alnum:]]{32})/??").unwrap();
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DatadogLogsConfig {
    address: SocketAddr,
    tls: Option<TlsConfig>,
    auth: Option<HttpSourceAuthConfig>,
    #[serde(default = "crate::serde::default_true")]
    store_api_key: bool,
}

inventory::submit! {
    SourceDescription::new::<DatadogLogsConfig>("datadog_logs")
}

impl GenerateConfig for DatadogLogsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:8080".parse().unwrap(),
            tls: None,
            auth: None,
            store_api_key: true,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_logs")]
impl SourceConfig for DatadogLogsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let source = DatadogLogsSource {
            store_api_key: self.store_api_key,
        };
        // We accept /v1/input & /v1/input/<API_KEY>
        source.run(self.address, "/v1/input", false, &self.tls, &self.auth, cx)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "datadog_logs"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }
}

#[derive(Clone, Default)]
struct DatadogLogsSource {
    store_api_key: bool,
}

#[derive(Deserialize)]
struct LogMsg {
    pub message: String,
    pub timestamp: i64,
}

impl HttpSource for DatadogLogsSource {
    fn build_events(
        &self,
        body: Bytes,
        header_map: HeaderMap,
        _query_parameters: HashMap<String, String>,
        request_path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        if body.is_empty() {
            // The datadog agent may send an empty payload as a keep alive
            debug!(
                message = "Empty payload ignored.",
                internal_log_rate_secs = 30
            );
            return Ok(Vec::new());
        }

        let api_key = match self.store_api_key {
            true => extract_api_key(&header_map, request_path),
            false => None,
        }
        .map(Arc::from);

        let messages: Vec<LogMsg> = serde_json::from_slice(&body).map_err(|error| {
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Error parsing JSON: {:?}", error),
            )
        })?;
        messages
            .into_iter()
            .map(|msg| {
                let mut log = LogEvent::default();
                log.insert(log_schema().timestamp_key(), Utc::now()); // Add timestamp
                log.insert_flat("message".to_string(), msg.message);
                log.insert_flat("timestamp".to_string(), msg.timestamp);
                let key = log_schema().source_type_key();
                log.try_insert(key, Bytes::from("datadog_logs"));
                if let Some(k) = &api_key {
                    log.metadata_mut().set_datadog_api_key(Some(Arc::clone(k)));
                }
                log
            })
            .map(|log| Ok(log.into()))
            .collect()
    }
}

fn extract_api_key<'a>(headers: &'a HeaderMap, path: &'a str) -> Option<String> {
    // Grab from URL first
    API_KEY_MATCHER
        .captures(path)
        .and_then(|cap| cap.name("api_key").map(|key| key.as_str()))
        // Try from header next
        .or_else(|| headers.get("dd-api-key").and_then(|key| key.to_str().ok()))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::DatadogLogsConfig;

    use crate::{
        config::{log_schema, SourceConfig, SourceContext},
        event::{Event, EventStatus},
        test_util::{next_addr, spawn_collect_n, trace_init, wait_for_tcp},
        Pipeline,
    };
    use futures::Stream;
    use http::HeaderMap;
    use pretty_assertions::assert_eq;
    use std::net::SocketAddr;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogLogsConfig>();
    }

    async fn source(
        status: EventStatus,
        acknowledgements: bool,
        store_api_key: bool,
    ) -> (impl Stream<Item = Event>, SocketAddr) {
        let (sender, recv) = Pipeline::new_test_finalize(status);
        let address = next_addr();
        let mut context = SourceContext::new_test(sender);
        context.acknowledgements = acknowledgements;
        tokio::spawn(async move {
            DatadogLogsConfig {
                address,
                tls: None,
                auth: None,
                store_api_key,
            }
            .build(context)
            .await
            .unwrap()
            .await
            .unwrap();
        });
        wait_for_tcp(address).await;
        (recv, address)
    }

    async fn send_with_path(
        address: SocketAddr,
        body: &str,
        headers: HeaderMap,
        path: &str,
    ) -> u16 {
        reqwest::Client::new()
            .post(&format!("http://{}{}", address, path))
            .headers(headers)
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    #[tokio::test]
    async fn no_api_key() {
        trace_init();
        let (rx, addr) = source(EventStatus::Delivered, true, true).await;

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        r#"[{"message":"foo", "timestamp": 123}]"#,
                        HeaderMap::new(),
                        "/v1/input/"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["message"], "foo".into());
            assert_eq!(log["timestamp"], 123.into());
            assert!(event.metadata().datadog_api_key().is_none());
            assert_eq!(log[log_schema().source_type_key()], "datadog_logs".into());
        }
    }

    #[tokio::test]
    async fn api_key_in_url() {
        trace_init();
        let (rx, addr) = source(EventStatus::Delivered, true, true).await;

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        r#"[{"message":"bar", "timestamp": 456}]"#,
                        HeaderMap::new(),
                        "/v1/input/12345678abcdefgh12345678abcdefgh"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["message"], "bar".into());
            assert_eq!(log["timestamp"], 456.into());
            assert_eq!(log[log_schema().source_type_key()], "datadog_logs".into());
            assert_eq!(
                &event.metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );
        }
    }

    #[tokio::test]
    async fn api_key_in_header() {
        trace_init();
        let (rx, addr) = source(EventStatus::Delivered, true, true).await;

        let mut headers = HeaderMap::new();
        headers.insert(
            "dd-api-key",
            "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
        );

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        r#"[{"message":"baz", "timestamp": 789}]"#,
                        headers,
                        "/v1/input/"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["message"], "baz".into());
            assert_eq!(log["timestamp"], 789.into());
            assert_eq!(log[log_schema().source_type_key()], "datadog_logs".into());
            assert_eq!(
                &event.metadata().datadog_api_key().as_ref().unwrap()[..],
                "12345678abcdefgh12345678abcdefgh"
            );
        }
    }

    #[tokio::test]
    async fn delivery_failure() {
        trace_init();
        let (rx, addr) = source(EventStatus::Failed, true, true).await;

        spawn_collect_n(
            async move {
                assert_eq!(
                    400,
                    send_with_path(
                        addr,
                        r#"[{"message":"foo", "timestamp": 123}]"#,
                        HeaderMap::new(),
                        "/v1/input/"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;
    }

    #[tokio::test]
    async fn ignores_disabled_acknowledgements() {
        trace_init();
        let (rx, addr) = source(EventStatus::Failed, false, true).await;

        let events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        r#"[{"message":"foo", "timestamp": 123}]"#,
                        HeaderMap::new(),
                        "/v1/input/"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn ignores_api_key() {
        trace_init();
        let (rx, addr) = source(EventStatus::Delivered, true, false).await;

        let mut headers = HeaderMap::new();
        headers.insert(
            "dd-api-key",
            "12345678abcdefgh12345678abcdefgh".parse().unwrap(),
        );

        let mut events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(
                        addr,
                        r#"[{"message":"baz", "timestamp": 789}]"#,
                        headers,
                        "/v1/input/12345678abcdefgh12345678abcdefgh"
                    )
                    .await
                );
            },
            rx,
            1,
        )
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["message"], "baz".into());
            assert_eq!(log["timestamp"], 789.into());
            assert_eq!(log[log_schema().source_type_key()], "datadog_logs".into());
            assert!(event.metadata().datadog_api_key().is_none());
        }
    }
}
