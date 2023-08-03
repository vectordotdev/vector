use std::collections::{HashMap, HashSet};

use bytes::{Bytes, BytesMut};
use futures::SinkExt;
use http::{Request, Uri};
use vrl::value::Kind;

use lookup::PathPrefix;
use vector_config::configurable_component;
use vector_core::schema;

use crate::sinks::cnosdb::{
    build_line_protocol, get_ts_from_value, value_to_line_string, CnosDBSettings, TYPE_TAG_KEY,
};
use crate::{
    codecs::Transformer,
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    event::Event,
    http::HttpClient,
    sinks::{
        cnosdb::healthcheck,
        util::{
            http::{BatchedHttpSink, HttpEventEncoder, HttpSink},
            BatchConfig, Buffer, Compression, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    tls::{TlsConfig, TlsSettings},
};

pub const DEFAULT_NAMESPACE: &str = "service";
pub const DEFAULT_TABLE: &str = "vector-logs";

pub const HOST_KEY: &str = "host";
pub const MESSAGE_KEY: &str = "message";
pub const TIMESTAMP_KEY: &str = "timestamp";
pub const TYPE_TAG_VALUE: &str = "logs";

#[derive(Clone, Copy, Debug, Default)]
pub struct CnosDBLogsDefaultBatchSettings;

impl SinkBatchSettings for CnosDBLogsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(1_000_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `cnosdb_logs` sink.
#[configurable_component(sink("cnosdb_logs", "Deliver log event data to CnosDB."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct CnosDBLogsConfig {
    /// The namespace of the table name to use.
    ///
    /// When specified, the table name is `<namespace>.vector-logs`.
    ///
    #[configurable(metadata(docs::examples = "service"))]
    pub namespace: Option<String>,

    ///  When specified, the table name is `service.<table>`.
    #[configurable(metadata(docs::examples = "vector-logs"))]
    pub table: Option<String>,

    /// The endpoint to send data to.
    ///
    /// This should be a full HTTP URI, including the scheme, host, and port.
    #[configurable(metadata(docs::examples = "http://localhost:8902/"))]
    pub endpoint: String,

    /// The list of names of log fields that should be added as tags to each table.
    ///
    /// By default Vector adds `metric_type` as well as the configured `log_schema.host_key` and
    /// `log_schema.source_type_key` options.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "field1"))]
    #[configurable(metadata(docs::examples = "parent.child_field"))]
    pub tags: Vec<String>,

    #[serde(flatten)]
    pub settings: CnosDBSettings,

    #[configurable(derived)]
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: Transformer,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<CnosDBLogsDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

impl CnosDBLogsConfig {
    fn get_table_name(&self) -> String {
        let namespace = self.namespace.as_deref().unwrap_or(DEFAULT_NAMESPACE);
        let table = self.table.as_deref().unwrap_or(DEFAULT_TABLE);
        format!("{}.{}", namespace, table)
    }

    fn healthcheck(&self, client: HttpClient) -> crate::Result<Healthcheck> {
        healthcheck(self.endpoint.clone(), self.settings.clone(), client)
    }
}

#[derive(Debug)]
struct CnosDBLogsSink {
    uri: Uri,
    table: String,
    tags: HashSet<String>,
    transformer: Transformer,
    auth: String,
}

impl_generate_config_from_default!(CnosDBLogsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "cnosdb_logs")]
impl SinkConfig for CnosDBLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let table = self.get_table_name();
        let tags: HashSet<String> = self.tags.clone().into_iter().collect();

        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;
        let healthcheck = self.healthcheck(client.clone())?;

        let batch = self.batch.into_batch_settings()?;
        let request = self.request.unwrap_with(&TowerRequestConfig {
            retry_attempts: Some(5),
            ..Default::default()
        });

        let endpoint = self.endpoint.clone();
        let uri = self.settings.write_uri(endpoint).unwrap();

        let auth = self.settings.authorization();

        let sink = CnosDBLogsSink {
            uri,
            auth,
            table,
            tags,
            transformer: self.encoding.clone(),
        };

        let sink = BatchedHttpSink::new(
            sink,
            Buffer::new(batch.size, Compression::None),
            request,
            batch.timeout,
            client,
        )
        .sink_map_err(|error| error!(message = "Fatal cnosdb_logs sink error.", %error));

        #[allow(deprecated)]
        Ok((VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        let requirements = schema::Requirement::empty()
            .optional_meaning(MESSAGE_KEY, Kind::bytes())
            .optional_meaning(HOST_KEY, Kind::bytes())
            .optional_meaning(TIMESTAMP_KEY, Kind::timestamp());

        Input::log().with_schema_requirement(requirements)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

struct CnosDBLogsEncoder {
    table: String,
    tags: HashSet<String>,
    transformer: Transformer,
}

impl HttpEventEncoder<BytesMut> for CnosDBLogsEncoder {
    fn encode_event(&mut self, event: Event) -> Option<BytesMut> {
        let mut log = event.into_log();
        if let Some(message_path) = log.message_path().cloned().as_ref() {
            log.rename_key(message_path, (PathPrefix::Event, MESSAGE_KEY))
        }
        // Add the `host` and `source_type` to the HashSet of tags to include
        // Ensure those paths are on the event to be encoded, rather than metadata
        if let Some(host_path) = log.host_path().cloned().as_ref() {
            self.tags.replace(host_path.path.to_string());
            log.rename_key(host_path, (PathPrefix::Event, HOST_KEY));
        }

        self.tags.replace(TYPE_TAG_KEY.to_string());
        log.insert(TYPE_TAG_KEY, TYPE_TAG_VALUE);

        // Timestamp
        let timestamp = get_ts_from_value(log.remove_timestamp());

        let log = {
            let mut event = Event::from(log);
            self.transformer.transform(&mut event);
            event.into_log()
        };

        // Tags + Fields
        let mut tags: HashMap<String, String> = HashMap::new();
        let mut fields: HashMap<String, String> = HashMap::new();
        log.convert_to_fields().for_each(|(key, value)| {
            if self.tags.contains(&key) {
                tags.insert(key, value.to_string_lossy().to_string().replace(' ', "_"));
            } else {
                fields.insert(key, value_to_line_string(value));
            }
        });

        let output =
            BytesMut::from(build_line_protocol(&self.table, tags, fields, timestamp).as_str());

        Some(output)
    }
}

#[async_trait::async_trait]
impl HttpSink for CnosDBLogsSink {
    type Input = BytesMut;
    type Output = BytesMut;
    type Encoder = CnosDBLogsEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        CnosDBLogsEncoder {
            table: self.table.clone(),
            tags: self.tags.clone(),
            transformer: self.transformer.clone(),
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Bytes>> {
        Request::post(&self.uri)
            .header("Content-Type", "text/plain")
            .header("Authorization", self.auth.clone())
            .body(events.freeze())
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod test {
    use crate::sinks::cnosdb::logs::CnosDBLogsSink;
    use crate::sinks::util::http::{HttpEventEncoder, HttpSink};
    use chrono::{TimeZone, Timelike, Utc};
    use http::Uri;
    use std::collections::HashSet;
    use vector_core::event::{Event, LogEvent};

    fn create_sink(uri: &str, auth: &str, table: &str, tags: Vec<&str>) -> CnosDBLogsSink {
        let uri = uri.parse::<Uri>().unwrap();
        let tags: HashSet<String> = tags.into_iter().map(|tag| tag.to_string()).collect();
        CnosDBLogsSink {
            uri,
            table: table.to_string(),
            tags,
            transformer: Default::default(),
            auth: auth.to_string(),
        }
    }

    #[test]
    fn test_cnosdb_sink_encode_event() {
        let mut event = Event::Log(LogEvent::from("hello world"));

        event.as_mut_log().insert(
            "timestamp",
            Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                .single()
                .and_then(|t| t.with_nanosecond(11))
                .expect("invalid timestamp"),
        );

        let sinks = create_sink(
            "http://localhost:8902",
            "Basic 11111",
            "vector-log",
            vec!["metric_type"],
        );
        let mut encoder = sinks.build_encoder();

        let bytes = encoder.encode_event(event).unwrap();
        let string = std::str::from_utf8(&bytes).unwrap();

        assert_eq!(
            string,
            "vector-log,metric_type=logs message=\"hello world\" 1542182950000000011\n"
        )
    }
}

#[cfg(feature = "cnosdb-integration-tests")]
#[cfg(test)]
mod test_integration {
    use crate::config::{SinkConfig, SinkContext};
    use crate::sinks::cnosdb::logs::CnosDBLogsConfig;
    use crate::test_util::components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS};
    use chrono::{TimeZone, Timelike, Utc};
    use futures_util::stream;
    use http::header::{ACCEPT, AUTHORIZATION};
    use vector_core::event::{Event, LogEvent};

    fn create_event(message: &str, i: u32) -> Event {
        let mut event = Event::Log(LogEvent::from(message));
        event.as_mut_log().insert(
            "timestamp",
            Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                .single()
                .and_then(|t| t.with_nanosecond(i))
                .expect("invalid timestamp"),
        );
        event
    }

    #[tokio::test]
    async fn test_cnosdb_sink() {
        let mut config = CnosDBLogsConfig::default();
        config.endpoint =
            std::env::var("CNOSDB_ENDPOINT").unwrap_or("http://127.0.0.1:8902/".to_string());
        let endpoint = if !config.endpoint.ends_with("/") {
            format!("{}/", config.endpoint)
        } else {
            config.endpoint.clone()
        };
        let (sink, _healthcheck) = config.build(SinkContext::default()).await.unwrap();
        let events = vec![
            create_event("hello world1", 1),
            create_event("hello world2", 2),
            create_event("hello world3", 3),
        ];
        run_and_assert_sink_compliance(sink, stream::iter(events), &HTTP_SINK_TAGS).await;
        let url = format!("{}api/v1/sql?tenant=cnosdb&db=public", endpoint);
        let client = reqwest::Client::new();
        let response = client
            .post(url)
            .header(AUTHORIZATION, "Basic cm9vdDo=")
            .header(ACCEPT, "application/table")
            .body(hyper::Body::from("SELECT * FROM \"service.vector-logs\""))
            .send()
            .await
            .unwrap();
        let res = response.text().await.unwrap();
        assert_eq!(
            res,
            "+-------------------------------+-------------+--------------+
             | time                          | metric_type | message      |
             +-------------------------------+-------------+--------------+
             | 2018-11-14T08:09:10.000000001 | logs        | hello world1 |
             | 2018-11-14T08:09:10.000000002 | logs        | hello world2 |
             | 2018-11-14T08:09:10.000000003 | logs        | hello world3 |
             +-------------------------------+-------------+--------------+"
        )
    }
}
