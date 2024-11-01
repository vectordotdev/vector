use std::collections::{HashMap, HashSet};

use bytes::{Bytes, BytesMut};
use futures::SinkExt;
use http::{Request, Uri};
use indoc::indoc;
use vrl::event_path;
use vrl::path::OwnedValuePath;
use vrl::value::Kind;

use vector_lib::config::log_schema;
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::lookup_v2::OptionalValuePath;
use vector_lib::lookup::PathPrefix;
use vector_lib::schema;

use super::{
    encode_timestamp, healthcheck, influx_line_protocol, influxdb_settings, Field,
    InfluxDb1Settings, InfluxDb2Settings, ProtocolVersion,
};
use crate::{
    codecs::Transformer,
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    event::{Event, KeyString, MetricTags, Value},
    http::HttpClient,
    internal_events::InfluxdbEncodingError,
    sinks::{
        util::{
            http::{BatchedHttpSink, HttpEventEncoder, HttpSink},
            BatchConfig, Buffer, Compression, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    tls::{TlsConfig, TlsSettings},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct InfluxDbLogsDefaultBatchSettings;

impl SinkBatchSettings for InfluxDbLogsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = Some(1_000_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `influxdb_logs` sink.
#[configurable_component(sink("influxdb_logs", "Deliver log event data to InfluxDB."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct InfluxDbLogsConfig {
    /// The namespace of the measurement name to use.
    ///
    /// When specified, the measurement name is `<namespace>.vector`.
    ///
    #[configurable(
        deprecated = "This field is deprecated, and `measurement` should be used instead."
    )]
    #[configurable(metadata(docs::examples = "service"))]
    pub namespace: Option<String>,

    /// The name of the InfluxDB measurement that is written to.
    #[configurable(metadata(docs::examples = "vector-logs"))]
    pub measurement: Option<String>,

    /// The endpoint to send data to.
    ///
    /// This should be a full HTTP URI, including the scheme, host, and port.
    #[configurable(metadata(docs::examples = "http://localhost:8086"))]
    pub endpoint: String,

    /// The list of names of log fields that should be added as tags to each measurement.
    ///
    /// By default Vector adds `metric_type` as well as the configured `log_schema.host_key` and
    /// `log_schema.source_type_key` options.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "field1"))]
    #[configurable(metadata(docs::examples = "parent.child_field"))]
    pub tags: Vec<KeyString>,

    #[serde(flatten)]
    pub influxdb1_settings: Option<InfluxDb1Settings>,

    #[serde(flatten)]
    pub influxdb2_settings: Option<InfluxDb2Settings>,

    #[configurable(derived)]
    #[serde(skip_serializing_if = "crate::serde::is_default", default)]
    pub encoding: Transformer,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<InfluxDbLogsDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    // `host_key`, `message_key`, and `source_type_key` are `Option` as we want `vector generate`
    // to produce a config with these as `None`, to not accidentally override a users configured
    // `log_schema`. Generating is constrained by build-time and can't account for changes to the
    // default `log_schema`.
    /// Use this option to customize the key containing the hostname.
    ///
    /// The setting of `log_schema.host_key`, usually `host`, is used here by default.
    #[configurable(metadata(docs::examples = "hostname"))]
    pub host_key: Option<OptionalValuePath>,

    /// Use this option to customize the key containing the message.
    ///
    /// The setting of `log_schema.message_key`, usually `message`, is used here by default.
    #[configurable(metadata(docs::examples = "text"))]
    pub message_key: Option<OptionalValuePath>,

    /// Use this option to customize the key containing the source_type.
    ///
    /// The setting of `log_schema.source_type_key`, usually `source_type`, is used here by default.
    #[configurable(metadata(docs::examples = "source"))]
    pub source_type_key: Option<OptionalValuePath>,
}

#[derive(Debug)]
struct InfluxDbLogsSink {
    uri: Uri,
    token: String,
    protocol_version: ProtocolVersion,
    measurement: String,
    tags: HashSet<KeyString>,
    transformer: Transformer,
    host_key: OwnedValuePath,
    message_key: OwnedValuePath,
    source_type_key: OwnedValuePath,
}

impl GenerateConfig for InfluxDbLogsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            endpoint = "http://localhost:8086/"
            namespace = "my-namespace"
            tags = []
            org = "my-org"
            bucket = "my-bucket"
            token = "${INFLUXDB_TOKEN}"
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "influxdb_logs")]
impl SinkConfig for InfluxDbLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let measurement = self.get_measurement()?;
        let tags: HashSet<KeyString> = self.tags.iter().cloned().collect();

        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;
        let healthcheck = self.healthcheck(client.clone())?;

        let batch = self.batch.into_batch_settings()?;
        let request = self.request.into_settings();

        let settings = influxdb_settings(
            self.influxdb1_settings.clone(),
            self.influxdb2_settings.clone(),
        )
        .unwrap();

        let endpoint = self.endpoint.clone();
        let uri = settings.write_uri(endpoint).unwrap();

        let token = settings.token();
        let protocol_version = settings.protocol_version();

        let host_key = self
            .host_key
            .as_ref()
            .and_then(|k| k.path.clone())
            .or_else(|| log_schema().host_key().cloned())
            .expect("global log_schema.host_key to be valid path");

        let message_key = self
            .message_key
            .as_ref()
            .and_then(|k| k.path.clone())
            .or_else(|| log_schema().message_key().cloned())
            .expect("global log_schema.message_key to be valid path");

        let source_type_key = self
            .source_type_key
            .as_ref()
            .and_then(|k| k.path.clone())
            .or_else(|| log_schema().source_type_key().cloned())
            .expect("global log_schema.source_type_key to be valid path");

        let sink = InfluxDbLogsSink {
            uri,
            token: token.inner().to_owned(),
            protocol_version,
            measurement,
            tags,
            transformer: self.encoding.clone(),
            host_key,
            message_key,
            source_type_key,
        };

        let sink = BatchedHttpSink::new(
            sink,
            Buffer::new(batch.size, Compression::None),
            request,
            batch.timeout,
            client,
        )
        .sink_map_err(|error| error!(message = "Fatal influxdb_logs sink error.", %error));

        #[allow(deprecated)]
        Ok((VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        let requirements = schema::Requirement::empty()
            .optional_meaning("message", Kind::bytes())
            .optional_meaning("host", Kind::bytes())
            .optional_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirements)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

struct InfluxDbLogsEncoder {
    protocol_version: ProtocolVersion,
    measurement: String,
    tags: HashSet<KeyString>,
    transformer: Transformer,
    host_key: OwnedValuePath,
    message_key: OwnedValuePath,
    source_type_key: OwnedValuePath,
}

impl HttpEventEncoder<BytesMut> for InfluxDbLogsEncoder {
    fn encode_event(&mut self, event: Event) -> Option<BytesMut> {
        let mut log = event.into_log();
        // If the event isn't an object (`. = "foo"`), inserting or renaming will result in losing
        // the original value that was assigned to the root. To avoid this we intentionally rename
        // the path that points to "message" such that it has a dedicated key.
        // TODO: add a `TargetPath::is_event_root()` to conditionally rename?
        if let Some(message_path) = log.message_path().cloned().as_ref() {
            log.rename_key(message_path, (PathPrefix::Event, &self.message_key));
        }
        // Add the `host` and `source_type` to the HashSet of tags to include
        // Ensure those paths are on the event to be encoded, rather than metadata
        if let Some(host_path) = log.host_path().cloned().as_ref() {
            self.tags.replace(host_path.path.to_string().into());
            log.rename_key(host_path, (PathPrefix::Event, &self.host_key));
        }

        if let Some(source_type_path) = log.source_type_path().cloned().as_ref() {
            self.tags.replace(source_type_path.path.to_string().into());
            log.rename_key(source_type_path, (PathPrefix::Event, &self.source_type_key));
        }

        self.tags.replace("metric_type".into());
        log.insert(event_path!("metric_type"), "logs");

        // Timestamp
        let timestamp = encode_timestamp(match log.remove_timestamp() {
            Some(Value::Timestamp(ts)) => Some(ts),
            _ => None,
        });

        let log = {
            let mut event = Event::from(log);
            self.transformer.transform(&mut event);
            event.into_log()
        };

        // Tags + Fields
        let mut tags = MetricTags::default();
        let mut fields: HashMap<KeyString, Field> = HashMap::new();
        log.convert_to_fields().for_each(|(key, value)| {
            if self.tags.contains(&key[..]) {
                tags.replace(key.into(), value.to_string_lossy().into_owned());
            } else {
                fields.insert(key, to_field(value));
            }
        });

        let mut output = BytesMut::new();
        if let Err(error_message) = influx_line_protocol(
            self.protocol_version,
            &self.measurement,
            Some(tags),
            Some(fields),
            timestamp,
            &mut output,
        ) {
            emit!(InfluxdbEncodingError {
                error_message,
                count: 1
            });
            return None;
        };

        Some(output)
    }
}

impl HttpSink for InfluxDbLogsSink {
    type Input = BytesMut;
    type Output = BytesMut;
    type Encoder = InfluxDbLogsEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        InfluxDbLogsEncoder {
            protocol_version: self.protocol_version,
            measurement: self.measurement.clone(),
            tags: self.tags.clone(),
            transformer: self.transformer.clone(),
            host_key: self.host_key.clone(),
            message_key: self.message_key.clone(),
            source_type_key: self.source_type_key.clone(),
        }
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Bytes>> {
        Request::post(&self.uri)
            .header("Content-Type", "text/plain")
            .header("Authorization", format!("Token {}", &self.token))
            .body(events.freeze())
            .map_err(Into::into)
    }
}

impl InfluxDbLogsConfig {
    fn get_measurement(&self) -> Result<String, &'static str> {
        match (self.measurement.as_ref(), self.namespace.as_ref()) {
            (Some(measure), Some(_)) => {
                warn!("Option `namespace` has been superseded by `measurement`.");
                Ok(measure.clone())
            }
            (Some(measure), None) => Ok(measure.clone()),
            (None, Some(namespace)) => {
                warn!(
                    "Option `namespace` has been deprecated. Use `measurement` instead. \
                       For example, you can use `measurement=<namespace>.vector` for the \
                       same effect."
                );
                Ok(format!("{}.vector", namespace))
            }
            (None, None) => Err("The `measurement` option is required."),
        }
    }

    fn healthcheck(&self, client: HttpClient) -> crate::Result<Healthcheck> {
        let config = self.clone();

        let healthcheck = healthcheck(
            config.endpoint,
            config.influxdb1_settings,
            config.influxdb2_settings,
            client,
        )?;

        Ok(healthcheck)
    }
}

fn to_field(value: &Value) -> Field {
    match value {
        Value::Integer(num) => Field::Int(*num),
        Value::Float(num) => Field::Float(num.into_inner()),
        Value::Boolean(b) => Field::Bool(*b),
        _ => Field::String(value.to_string_lossy().into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{offset::TimeZone, Utc};
    use futures::{channel::mpsc, stream, StreamExt};
    use http::{request::Parts, StatusCode};
    use indoc::indoc;

    use vector_lib::event::{BatchNotifier, BatchStatus, Event, LogEvent};
    use vector_lib::lookup::owned_value_path;

    use crate::{
        sinks::{
            influxdb::test_util::{assert_fields, split_line_protocol, ts},
            util::test::{build_test_server_status, load_sink},
        },
        test_util::{
            components::{
                run_and_assert_sink_compliance, run_and_assert_sink_error, COMPONENT_ERROR_TAGS,
                HTTP_SINK_TAGS,
            },
            next_addr,
        },
    };

    use super::*;

    type Receiver = mpsc::Receiver<(Parts, bytes::Bytes)>;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<InfluxDbLogsConfig>();
    }

    #[test]
    fn test_config_without_tags() {
        let config = indoc! {r#"
            namespace = "vector-logs"
            endpoint = "http://localhost:9999"
            bucket = "my-bucket"
            org = "my-org"
            token = "my-token"
        "#};

        toml::from_str::<InfluxDbLogsConfig>(config).unwrap();
    }

    #[test]
    fn test_config_measurement_from_namespace() {
        let config = indoc! {r#"
            namespace = "ns"
            endpoint = "http://localhost:9999"
        "#};

        let sink_config = toml::from_str::<InfluxDbLogsConfig>(config).unwrap();
        assert_eq!("ns.vector", sink_config.get_measurement().unwrap());
    }

    #[test]
    fn test_encode_event_apply_rules() {
        let mut event = Event::Log(LogEvent::from("hello"));
        event.as_mut_log().insert("host", "aws.cloud.eur");
        event.as_mut_log().insert("timestamp", ts());

        let mut sink = create_sink(
            "http://localhost:9999",
            "my-token",
            ProtocolVersion::V1,
            "vector",
            ["metric_type", "host"].to_vec(),
        );
        sink.transformer
            .set_except_fields(Some(vec!["host".into()]))
            .unwrap();
        let mut encoder = sink.build_encoder();

        let bytes = encoder.encode_event(event.clone()).unwrap();
        let string = std::str::from_utf8(&bytes).unwrap();

        let line_protocol = split_line_protocol(string);
        assert_eq!("vector", line_protocol.0);
        assert_eq!("metric_type=logs", line_protocol.1);
        assert_fields(line_protocol.2.to_string(), ["message=\"hello\""].to_vec());
        assert_eq!("1542182950000000011\n", line_protocol.3);

        sink.transformer
            .set_except_fields(Some(vec!["metric_type".into()]))
            .unwrap();
        let mut encoder = sink.build_encoder();
        let bytes = encoder.encode_event(event.clone()).unwrap();
        let string = std::str::from_utf8(&bytes).unwrap();
        let line_protocol = split_line_protocol(string);
        assert_eq!(
            "host=aws.cloud.eur", line_protocol.1,
            "metric_type tag should be excluded"
        );
        assert_fields(line_protocol.2, ["message=\"hello\""].to_vec());
    }

    #[test]
    fn test_encode_event_v1() {
        let mut event = Event::Log(LogEvent::from("hello"));
        event.as_mut_log().insert("host", "aws.cloud.eur");
        event.as_mut_log().insert("source_type", "file");

        event.as_mut_log().insert("int", 4i32);
        event.as_mut_log().insert("float", 5.5);
        event.as_mut_log().insert("bool", true);
        event.as_mut_log().insert("string", "thisisastring");
        event.as_mut_log().insert("timestamp", ts());

        let sink = create_sink(
            "http://localhost:9999",
            "my-token",
            ProtocolVersion::V1,
            "vector",
            ["source_type", "host", "metric_type"].to_vec(),
        );
        let mut encoder = sink.build_encoder();

        let bytes = encoder.encode_event(event).unwrap();
        let string = std::str::from_utf8(&bytes).unwrap();

        let line_protocol = split_line_protocol(string);
        assert_eq!("vector", line_protocol.0);
        assert_eq!(
            "host=aws.cloud.eur,metric_type=logs,source_type=file",
            line_protocol.1
        );
        assert_fields(
            line_protocol.2.to_string(),
            [
                "int=4i",
                "float=5.5",
                "bool=true",
                "string=\"thisisastring\"",
                "message=\"hello\"",
            ]
            .to_vec(),
        );

        assert_eq!("1542182950000000011\n", line_protocol.3);
    }

    #[test]
    fn test_encode_event() {
        let mut event = Event::Log(LogEvent::from("hello"));
        event.as_mut_log().insert("host", "aws.cloud.eur");
        event.as_mut_log().insert("source_type", "file");

        event.as_mut_log().insert("int", 4i32);
        event.as_mut_log().insert("float", 5.5);
        event.as_mut_log().insert("bool", true);
        event.as_mut_log().insert("string", "thisisastring");
        event.as_mut_log().insert("timestamp", ts());

        let sink = create_sink(
            "http://localhost:9999",
            "my-token",
            ProtocolVersion::V2,
            "vector",
            ["source_type", "host", "metric_type"].to_vec(),
        );
        let mut encoder = sink.build_encoder();

        let bytes = encoder.encode_event(event).unwrap();
        let string = std::str::from_utf8(&bytes).unwrap();

        let line_protocol = split_line_protocol(string);
        assert_eq!("vector", line_protocol.0);
        assert_eq!(
            "host=aws.cloud.eur,metric_type=logs,source_type=file",
            line_protocol.1
        );
        assert_fields(
            line_protocol.2.to_string(),
            [
                "int=4i",
                "float=5.5",
                "bool=true",
                "string=\"thisisastring\"",
                "message=\"hello\"",
            ]
            .to_vec(),
        );

        assert_eq!("1542182950000000011\n", line_protocol.3);
    }

    #[test]
    fn test_encode_event_without_tags() {
        let mut event = Event::Log(LogEvent::from("hello"));

        event.as_mut_log().insert("value", 100);
        event.as_mut_log().insert("timestamp", ts());

        let mut sink = create_sink(
            "http://localhost:9999",
            "my-token",
            ProtocolVersion::V2,
            "vector",
            [].to_vec(),
        );
        // exclude default metric_type tag so to emit empty tags
        sink.transformer
            .set_except_fields(Some(vec!["metric_type".into()]))
            .unwrap();
        let mut encoder = sink.build_encoder();

        let bytes = encoder.encode_event(event).unwrap();
        let line = std::str::from_utf8(&bytes).unwrap();
        assert!(
            line.starts_with("vector "),
            "measurement (without tags) should ends with space ' '"
        );

        let line_protocol = split_line_protocol(line);
        assert_eq!("vector", line_protocol.0);
        assert_eq!("", line_protocol.1, "tags should be empty");
        assert_fields(
            line_protocol.2,
            ["value=100i", "message=\"hello\""].to_vec(),
        );

        assert_eq!("1542182950000000011\n", line_protocol.3);
    }

    #[test]
    fn test_encode_nested_fields() {
        let mut event = LogEvent::default();

        event.insert("a", 1);
        event.insert("nested.field", "2");
        event.insert("nested.bool", true);
        event.insert("nested.array[0]", "example-value");
        event.insert("nested.array[2]", "another-value");
        event.insert("nested.array[3]", 15);

        let sink = create_sink(
            "http://localhost:9999",
            "my-token",
            ProtocolVersion::V2,
            "vector",
            ["metric_type"].to_vec(),
        );
        let mut encoder = sink.build_encoder();

        let bytes = encoder.encode_event(event.into()).unwrap();
        let string = std::str::from_utf8(&bytes).unwrap();

        let line_protocol = split_line_protocol(string);
        assert_eq!("vector", line_protocol.0);
        assert_eq!("metric_type=logs", line_protocol.1);
        assert_fields(
            line_protocol.2,
            [
                "a=1i",
                "nested.array[0]=\"example-value\"",
                "nested.array[1]=\"<null>\"",
                "nested.array[2]=\"another-value\"",
                "nested.array[3]=15i",
                "nested.bool=true",
                "nested.field=\"2\"",
            ]
            .to_vec(),
        );
    }

    #[test]
    fn test_add_tag() {
        let mut event = Event::Log(LogEvent::from("hello"));
        event.as_mut_log().insert("source_type", "file");

        event.as_mut_log().insert("as_a_tag", 10);
        event.as_mut_log().insert("timestamp", ts());

        let sink = create_sink(
            "http://localhost:9999",
            "my-token",
            ProtocolVersion::V2,
            "vector",
            ["as_a_tag", "not_exists_field", "source_type", "metric_type"].to_vec(),
        );
        let mut encoder = sink.build_encoder();

        let bytes = encoder.encode_event(event).unwrap();
        let string = std::str::from_utf8(&bytes).unwrap();

        let line_protocol = split_line_protocol(string);
        assert_eq!("vector", line_protocol.0);
        assert_eq!(
            "as_a_tag=10,metric_type=logs,source_type=file",
            line_protocol.1
        );
        assert_fields(line_protocol.2.to_string(), ["message=\"hello\""].to_vec());

        assert_eq!("1542182950000000011\n", line_protocol.3);
    }

    #[tokio::test]
    async fn smoke_v1() {
        let rx = smoke_test(
            r#"database = "my-database""#,
            StatusCode::OK,
            BatchStatus::Delivered,
        )
        .await;

        let query = receive_response(rx).await;
        assert!(query.contains("db=my-database"));
        assert!(query.contains("precision=ns"));
    }

    #[tokio::test]
    async fn smoke_v1_failure() {
        smoke_test(
            r#"database = "my-database""#,
            StatusCode::BAD_REQUEST,
            BatchStatus::Rejected,
        )
        .await;
    }

    #[tokio::test]
    async fn smoke_v2() {
        let rx = smoke_test(
            indoc! {r#"
            bucket = "my-bucket"
            org = "my-org"
            token = "my-token"
        "#},
            StatusCode::OK,
            BatchStatus::Delivered,
        )
        .await;

        let query = receive_response(rx).await;
        assert!(query.contains("org=my-org"));
        assert!(query.contains("bucket=my-bucket"));
        assert!(query.contains("precision=ns"));
    }

    #[tokio::test]
    async fn smoke_v2_failure() {
        smoke_test(
            indoc! {r#"
            bucket = "my-bucket"
            org = "my-org"
            token = "my-token"
        "#},
            StatusCode::BAD_REQUEST,
            BatchStatus::Rejected,
        )
        .await;
    }

    async fn smoke_test(
        config: &str,
        status_code: StatusCode,
        batch_status: BatchStatus,
    ) -> Receiver {
        let config = format!(
            indoc! {r#"
            measurement = "vector"
            endpoint = "http://localhost:9999"
            {}
        "#},
            config
        );
        let (mut config, cx) = load_sink::<InfluxDbLogsConfig>(&config).unwrap();

        // Make sure we can build the config
        _ = config.build(cx.clone()).await.unwrap();

        let addr = next_addr();
        // Swap out the host so we can force send it
        // to our local server
        let host = format!("http://{}", addr);
        config.endpoint = host;

        let (sink, _) = config.build(cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server_status(addr, status_code);
        tokio::spawn(server);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();

        let lines = std::iter::repeat(())
            .map(move |_| "message_value")
            .take(5)
            .collect::<Vec<_>>();
        let mut events = Vec::new();

        // Create 5 events with custom field
        for (i, line) in lines.iter().enumerate() {
            let mut event = LogEvent::from(line.to_string()).with_batch_notifier(&batch);
            event.insert(format!("key{}", i).as_str(), format!("value{}", i));

            let timestamp = Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, (i as u32) + 1)
                .single()
                .expect("invalid timestamp");
            event.insert("timestamp", timestamp);
            event.insert("source_type", "file");

            events.push(Event::Log(event));
        }
        drop(batch);

        if batch_status == BatchStatus::Delivered {
            run_and_assert_sink_compliance(sink, stream::iter(events), &HTTP_SINK_TAGS).await;
        } else {
            run_and_assert_sink_error(sink, stream::iter(events), &COMPONENT_ERROR_TAGS).await;
        }

        assert_eq!(receiver.try_recv(), Ok(batch_status));

        rx
    }

    async fn receive_response(mut rx: Receiver) -> String {
        let output = rx.next().await.unwrap();

        let request = &output.0;
        let query = request.uri.query().unwrap();

        let body = std::str::from_utf8(&output.1[..]).unwrap();
        let mut lines = body.lines();

        assert_eq!(5, lines.clone().count());
        assert_line_protocol(0, lines.next());

        query.into()
    }

    fn assert_line_protocol(i: i64, value: Option<&str>) {
        //vector,metric_type=logs key0="value0",message="message_value" 1000000000
        let line_protocol = split_line_protocol(value.unwrap());
        assert_eq!("vector", line_protocol.0);
        assert_eq!("metric_type=logs,source_type=file", line_protocol.1);
        assert_fields(
            line_protocol.2.to_string(),
            [
                &*format!("key{}=\"value{}\"", i, i),
                "message=\"message_value\"",
            ]
            .to_vec(),
        );

        assert_eq!(((i + 1) * 1000000000).to_string(), line_protocol.3);
    }

    fn create_sink(
        uri: &str,
        token: &str,
        protocol_version: ProtocolVersion,
        measurement: &str,
        tags: Vec<&str>,
    ) -> InfluxDbLogsSink {
        let uri = uri.parse::<Uri>().unwrap();
        let token = token.to_string();
        let measurement = measurement.to_string();
        let tags: HashSet<_> = tags.into_iter().map(|tag| tag.into()).collect();
        InfluxDbLogsSink {
            uri,
            token,
            protocol_version,
            measurement,
            tags,
            transformer: Default::default(),
            host_key: owned_value_path!("host"),
            message_key: owned_value_path!("message"),
            source_type_key: owned_value_path!("source_type"),
        }
    }
}

#[cfg(feature = "influxdb-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use std::sync::Arc;

    use chrono::Utc;
    use futures::stream;
    use vrl::value;

    use vector_lib::codecs::BytesDeserializerConfig;
    use vector_lib::config::{LegacyKey, LogNamespace};
    use vector_lib::event::{BatchNotifier, BatchStatus, Event, LogEvent};
    use vector_lib::lookup::{owned_value_path, path};

    use crate::{
        config::SinkContext,
        sinks::influxdb::{
            logs::InfluxDbLogsConfig,
            test_util::{address_v2, onboarding_v2, BUCKET, ORG, TOKEN},
            InfluxDb2Settings,
        },
        test_util::components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS},
    };

    use super::*;

    #[tokio::test]
    async fn influxdb2_logs_put_data() {
        let endpoint = address_v2();
        onboarding_v2(&endpoint).await;

        let now = Utc::now();
        let measure = format!(
            "vector-{}",
            now.timestamp_nanos_opt().expect("Timestamp out of range")
        );

        let cx = SinkContext::default();

        let config = InfluxDbLogsConfig {
            namespace: None,
            measurement: Some(measure.clone()),
            endpoint: endpoint.clone(),
            tags: Default::default(),
            influxdb1_settings: None,
            influxdb2_settings: Some(InfluxDb2Settings {
                org: ORG.to_string(),
                bucket: BUCKET.to_string(),
                token: TOKEN.to_string().into(),
            }),
            encoding: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            tls: None,
            acknowledgements: Default::default(),
            host_key: None,
            message_key: None,
            source_type_key: None,
        };

        let (sink, _) = config.build(cx).await.unwrap();

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();

        let mut event1 = LogEvent::from("message_1").with_batch_notifier(&batch);
        event1.insert("host", "aws.cloud.eur");
        event1.insert("source_type", "file");

        let mut event2 = LogEvent::from("message_2").with_batch_notifier(&batch);
        event2.insert("host", "aws.cloud.eur");
        event2.insert("source_type", "file");

        let mut namespaced_log =
            LogEvent::from(value!("namespaced message")).with_batch_notifier(&batch);
        LogNamespace::Vector.insert_source_metadata(
            "file",
            &mut namespaced_log,
            Some(LegacyKey::Overwrite(path!("host"))),
            path!("host"),
            "aws.cloud.eur",
        );
        LogNamespace::Vector.insert_standard_vector_source_metadata(
            &mut namespaced_log,
            "file",
            now,
        );
        let schema = BytesDeserializerConfig
            .schema_definition(LogNamespace::Vector)
            .with_metadata_field(
                &owned_value_path!("file", "host"),
                Kind::bytes(),
                Some("host"),
            );
        namespaced_log
            .metadata_mut()
            .set_schema_definition(&Arc::new(schema));

        drop(batch);

        let events = vec![
            Event::Log(event1),
            Event::Log(event2),
            Event::Log(namespaced_log),
        ];

        run_and_assert_sink_compliance(sink, stream::iter(events), &HTTP_SINK_TAGS).await;

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let mut body = std::collections::HashMap::new();
        body.insert("query", format!("from(bucket:\"my-bucket\") |> range(start: 0) |> filter(fn: (r) => r._measurement == \"{}\")", measure.clone()));
        body.insert("type", "flux".to_owned());

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let res = client
            .post(format!("{}/api/v2/query?org=my-org", endpoint))
            .json(&body)
            .header("accept", "application/json")
            .header("Authorization", "Token my-token")
            .send()
            .await
            .unwrap();
        let string = res.text().await.unwrap();

        let lines = string.split('\n').collect::<Vec<&str>>();
        let header = lines[0].split(',').collect::<Vec<&str>>();
        let record1 = lines[1].split(',').collect::<Vec<&str>>();
        let record2 = lines[2].split(',').collect::<Vec<&str>>();
        let record_ns = lines[3].split(',').collect::<Vec<&str>>();

        // measurement
        assert_eq!(
            record1[header
                .iter()
                .position(|&r| r.trim() == "_measurement")
                .unwrap()]
            .trim(),
            measure.clone()
        );
        assert_eq!(
            record2[header
                .iter()
                .position(|&r| r.trim() == "_measurement")
                .unwrap()]
            .trim(),
            measure.clone()
        );
        assert_eq!(
            record_ns[header
                .iter()
                .position(|&r| r.trim() == "_measurement")
                .unwrap()]
            .trim(),
            measure.clone()
        );

        // tags
        assert_eq!(
            record1[header
                .iter()
                .position(|&r| r.trim() == "metric_type")
                .unwrap()]
            .trim(),
            "logs"
        );
        assert_eq!(
            record2[header
                .iter()
                .position(|&r| r.trim() == "metric_type")
                .unwrap()]
            .trim(),
            "logs"
        );
        assert_eq!(
            record_ns[header
                .iter()
                .position(|&r| r.trim() == "metric_type")
                .unwrap()]
            .trim(),
            "logs"
        );
        assert_eq!(
            record1[header.iter().position(|&r| r.trim() == "host").unwrap()].trim(),
            "aws.cloud.eur"
        );
        assert_eq!(
            record2[header.iter().position(|&r| r.trim() == "host").unwrap()].trim(),
            "aws.cloud.eur"
        );
        assert_eq!(
            record_ns[header.iter().position(|&r| r.trim() == "host").unwrap()].trim(),
            "aws.cloud.eur"
        );
        assert_eq!(
            record1[header
                .iter()
                .position(|&r| r.trim() == "source_type")
                .unwrap()]
            .trim(),
            "file"
        );
        assert_eq!(
            record2[header
                .iter()
                .position(|&r| r.trim() == "source_type")
                .unwrap()]
            .trim(),
            "file"
        );
        assert_eq!(
            record_ns[header
                .iter()
                .position(|&r| r.trim() == "source_type")
                .unwrap()]
            .trim(),
            "file"
        );

        // field
        assert_eq!(
            record1[header.iter().position(|&r| r.trim() == "_field").unwrap()].trim(),
            "message"
        );
        assert_eq!(
            record2[header.iter().position(|&r| r.trim() == "_field").unwrap()].trim(),
            "message"
        );
        assert_eq!(
            record_ns[header.iter().position(|&r| r.trim() == "_field").unwrap()].trim(),
            "message"
        );
        assert_eq!(
            record1[header.iter().position(|&r| r.trim() == "_value").unwrap()].trim(),
            "message_1"
        );
        assert_eq!(
            record2[header.iter().position(|&r| r.trim() == "_value").unwrap()].trim(),
            "message_2"
        );
        assert_eq!(
            record_ns[header.iter().position(|&r| r.trim() == "_value").unwrap()].trim(),
            "namespaced message"
        );
    }
}
