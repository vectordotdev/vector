use serde::{Deserialize, Serialize};

use super::{host_key, Encoding};
use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    sinks::{
        splunk_hec::{
            common::{
                acknowledgements::HecClientAcknowledgementsConfig, SplunkHecDefaultBatchSettings,
            },
            logs::config::HecLogsSinkConfig,
        },
        util::{encoding::EncodingConfig, BatchConfig, Compression, TowerRequestConfig},
        Healthcheck, VectorSink,
    },
    template::Template,
    tls::TlsOptions,
};

const HOST: &str = "https://cloud.humio.com";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HumioLogsConfig {
    pub(in crate::sinks::humio) token: String,
    // Deprecated name
    #[serde(alias = "host")]
    pub(in crate::sinks::humio) endpoint: Option<String>,
    pub(in crate::sinks::humio) source: Option<Template>,
    pub(in crate::sinks::humio) encoding: EncodingConfig<Encoding>,
    pub(in crate::sinks::humio) event_type: Option<Template>,
    #[serde(default = "host_key")]
    pub(in crate::sinks::humio) host_key: String,
    #[serde(default)]
    pub(in crate::sinks::humio) indexed_fields: Vec<String>,
    #[serde(default)]
    pub(in crate::sinks::humio) index: Option<Template>,
    #[serde(default)]
    pub(in crate::sinks::humio) compression: Compression,
    #[serde(default)]
    pub(in crate::sinks::humio) request: TowerRequestConfig,
    #[serde(default)]
    pub(in crate::sinks::humio) batch: BatchConfig<SplunkHecDefaultBatchSettings>,
    pub(in crate::sinks::humio) tls: Option<TlsOptions>,
    #[serde(default = "timestamp_nanos_key")]
    pub(in crate::sinks::humio) timestamp_nanos_key: Option<String>,
}

inventory::submit! {
    SinkDescription::new::<HumioLogsConfig>("humio_logs")
}

pub fn timestamp_nanos_key() -> Option<String> {
    Some("@timestamp.nanos".to_string())
}

impl GenerateConfig for HumioLogsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            token: "${HUMIO_TOKEN}".to_owned(),
            endpoint: None,
            source: None,
            encoding: Encoding::Json.into(),
            event_type: None,
            indexed_fields: vec![],
            index: None,
            host_key: host_key(),
            compression: Compression::default(),
            request: TowerRequestConfig::default(),
            batch: BatchConfig::default(),
            tls: None,
            timestamp_nanos_key: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "humio_logs")]
impl SinkConfig for HumioLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        self.build_hec_config().build(cx).await
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "humio_logs"
    }
}

impl HumioLogsConfig {
    fn build_hec_config(&self) -> HecLogsSinkConfig {
        let endpoint = self.endpoint.clone().unwrap_or_else(|| HOST.to_string());

        HecLogsSinkConfig {
            default_token: self.token.clone(),
            endpoint,
            host_key: self.host_key.clone(),
            indexed_fields: self.indexed_fields.clone(),
            index: self.index.clone(),
            sourcetype: self.event_type.clone(),
            source: self.source.clone(),
            timestamp_nanos_key: self.timestamp_nanos_key.clone(),
            encoding: self.encoding.clone().into_encoding(),
            compression: self.compression,
            batch: self.batch,
            request: self.request,
            tls: self.tls.clone(),
            acknowledgements: HecClientAcknowledgementsConfig {
                indexer_acknowledgements_enabled: false,
                ..Default::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<HumioLogsConfig>();
    }
}

#[cfg(test)]
#[cfg(feature = "humio-integration-tests")]
mod integration_tests {
    use chrono::{TimeZone, Utc};
    use indoc::indoc;
    use serde_json::{json, Value as JsonValue};
    use std::{collections::HashMap, convert::TryFrom};
    use tokio::time::Duration;

    use super::*;
    use crate::{
        config::{log_schema, SinkConfig, SinkContext},
        event::Event,
        sinks::util::Compression,
        test_util::{components, components::HTTP_SINK_TAGS, random_string},
    };

    fn humio_address() -> String {
        std::env::var("HUMIO_ADDRESS").unwrap_or_else(|_| "http://localhost:8080".into())
    }

    #[tokio::test]
    async fn humio_insert_message() {
        wait_ready().await;

        let cx = SinkContext::new_test();

        let repo = create_repository().await;

        let config = config(&repo.default_ingest_token);

        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let host = "192.168.1.1".to_string();
        let mut event = Event::from(message.clone());
        let log = event.as_mut_log();
        log.insert(log_schema().host_key(), host.clone());

        let ts = Utc.timestamp_nanos(Utc::now().timestamp_millis() * 1_000_000 + 132_456);
        log.insert(log_schema().timestamp_key(), ts);

        components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;

        let entry = find_entry(repo.name.as_str(), message.as_str()).await;

        assert_eq!(
            message,
            entry
                .fields
                .get("message")
                .expect("no message key")
                .as_str()
                .unwrap()
        );
        assert!(
            entry.error.is_none(),
            "Humio encountered an error parsing this message: {}",
            entry
                .error_msg
                .unwrap_or_else(|| "no error message".to_string())
        );
        assert_eq!(Some(host), entry.host);
        assert_eq!("132456", entry.timestamp_nanos);
    }

    #[tokio::test]
    async fn humio_insert_source() {
        wait_ready().await;

        let cx = SinkContext::new_test();

        let repo = create_repository().await;

        let mut config = config(&repo.default_ingest_token);
        config.source = Template::try_from("/var/log/syslog".to_string()).ok();

        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let event = Event::from(message.clone());
        components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;

        let entry = find_entry(repo.name.as_str(), message.as_str()).await;

        assert_eq!(entry.source, Some("/var/log/syslog".to_owned()));
        assert!(
            entry.error.is_none(),
            "Humio encountered an error parsing this message: {}",
            entry
                .error_msg
                .unwrap_or_else(|| "no error message".to_string())
        );
    }

    #[tokio::test]
    async fn humio_type() {
        wait_ready().await;

        let repo = create_repository().await;

        // sets type
        {
            let mut config = config(&repo.default_ingest_token);
            config.event_type = Template::try_from("json".to_string()).ok();

            let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

            let message = random_string(100);
            let mut event = Event::from(message.clone());
            // Humio expects to find an @timestamp field for JSON lines
            // https://docs.humio.com/ingesting-data/parsers/built-in-parsers/#json
            event
                .as_mut_log()
                .insert("@timestamp", Utc::now().to_rfc3339());

            components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;

            let entry = find_entry(repo.name.as_str(), message.as_str()).await;

            assert_eq!(entry.humio_type, "json");
            assert!(
                entry.error.is_none(),
                "Humio encountered an error parsing this message: {}",
                entry
                    .error_msg
                    .unwrap_or_else(|| "no error message".to_string())
            );
        }

        // defaults to none
        {
            let config = config(&repo.default_ingest_token);

            let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

            let message = random_string(100);
            let event = Event::from(message.clone());

            components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;

            let entry = find_entry(repo.name.as_str(), message.as_str()).await;

            assert_eq!(entry.humio_type, "none");
        }
    }

    /// create a new test config with the given ingest token
    fn config(token: &str) -> super::HumioLogsConfig {
        let mut batch = BatchConfig::default();
        batch.max_events = Some(1);

        HumioLogsConfig {
            token: token.to_string(),
            endpoint: Some(humio_address()),
            source: None,
            encoding: Encoding::Json.into(),
            event_type: None,
            host_key: log_schema().host_key().to_string(),
            indexed_fields: vec![],
            index: None,
            compression: Compression::None,
            request: TowerRequestConfig::default(),
            batch,
            tls: None,
            timestamp_nanos_key: timestamp_nanos_key(),
        }
    }

    async fn wait_ready() {
        crate::test_util::retry_until(
            || async {
                reqwest::get(format!("{}/api/v1/status", humio_address()))
                    .await
                    .map_err(|err| err.to_string())
                    .and_then(|res| {
                        if res.status().is_success() {
                            Ok(())
                        } else {
                            Err("server not ready...".into())
                        }
                    })
            },
            Duration::from_secs(1),
            Duration::from_secs(30),
        )
        .await;
    }

    /// create a new test humio repository to publish to
    async fn create_repository() -> HumioRepository {
        let client = reqwest::Client::builder().build().unwrap();

        // https://docs.humio.com/api/graphql/
        let graphql_url = format!("{}/graphql", humio_address());

        let name = random_string(50);

        let params = json!({
        "query": format!(
            indoc!{ r#"
                mutation {{
                  createRepository(name:"{}") {{
                    repository {{
                      name
                      type
                      ingestTokens {{
                        name
                        token
                      }}
                    }}
                  }}
                }}
            "#},
            name
        ),
        });

        let res = client
            .post(&graphql_url)
            .json(&params)
            .send()
            .await
            .unwrap();

        let json: JsonValue = res.json().await.unwrap();
        let repository = &json["data"]["createRepository"]["repository"];

        let token = repository["ingestTokens"].as_array().unwrap()[0]["token"]
            .as_str()
            .unwrap()
            .to_string();

        HumioRepository {
            name: repository["name"].as_str().unwrap().to_string(),
            default_ingest_token: token,
        }
    }

    /// fetch event from the repository that has a matching message value
    async fn find_entry(repository_name: &str, message: &str) -> HumioLog {
        let client = reqwest::Client::builder().build().unwrap();

        // https://docs.humio.com/api/using-the-search-api-with-humio
        let search_url = format!(
            "{}/api/v1/repositories/{}/query",
            humio_address(),
            repository_name
        );
        let search_query = format!(r#"message="{}""#, message);

        // events are not available to search API immediately
        // poll up 20 times for event to show up
        for _ in 0..20usize {
            let res = client
                .post(&search_url)
                .json(&json!({
                    "queryString": search_query,
                }))
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .await
                .unwrap();

            let logs: Vec<HumioLog> = res.json().await.unwrap();

            if !logs.is_empty() {
                return logs[0].clone();
            }
        }
        panic!(
            "did not find event in Humio repository {} with message {}",
            repository_name, message
        );
    }

    #[derive(Debug)]
    struct HumioRepository {
        name: String,
        default_ingest_token: String,
    }

    #[derive(Clone, Deserialize)]
    #[allow(dead_code)] // deserialize all fields
    struct HumioLog {
        #[serde(rename = "#repo")]
        humio_repo: String,

        #[serde(rename = "#type")]
        humio_type: String,

        #[serde(rename = "@error")]
        error: Option<String>,

        #[serde(rename = "@error_msg")]
        error_msg: Option<String>,

        #[serde(rename = "@rawstring")]
        rawstring: String,

        #[serde(rename = "@id")]
        id: String,

        #[serde(rename = "@timestamp")]
        timestamp_millis: u64,

        #[serde(rename = "@timestamp.nanos")]
        timestamp_nanos: String,

        #[serde(rename = "@timezone")]
        timezone: String,

        #[serde(rename = "@source")]
        source: Option<String>,

        #[serde(rename = "@host")]
        host: Option<String>,

        // fields parsed from ingested log
        #[serde(flatten)]
        fields: HashMap<String, JsonValue>,
    }
}
