#![expect(
    clippy::let_underscore_must_use,
    reason = "derivative's Debug derive with ignored fields expands to a must_use let binding"
)]

use std::collections::{HashMap, HashSet};

use bytes::{Bytes, BytesMut};
use derivative::Derivative;
use futures::SinkExt;
use http::{Request, Uri};
use indoc::indoc;
use vector_lib::{
    config::log_schema,
    configurable::configurable_component,
    lookup::{PathPrefix, lookup_v2::OptionalValuePath},
    schema,
    sensitive_string::SensitiveString,
};
use vrl::{event_path, path::OwnedValuePath, value::Kind};

use super::{
    Field, InfluxDb1Settings, InfluxDb2Settings, InfluxDbSettings, InfluxDbVersion,
    ProtocolVersion, encode_timestamp, healthcheck, influx_line_protocol, influxdb_settings,
};
use crate::{
    codecs::Transformer,
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    event::{Event, KeyString, MetricTags, Value},
    http::HttpClient,
    internal_events::InfluxdbEncodingError,
    sinks::{
        Healthcheck, VectorSink,
        util::{
            BatchConfig, BatchSettings, Buffer, Compression, HttpEndpoint, SinkBatchSettings,
            TowerRequestConfig,
            http::{BatchedHttpSink, HttpEventEncoder, HttpSink},
        },
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
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct InfluxDbLogsConfig {
    /// The name of the InfluxDB measurement that is written to.
    #[configurable(metadata(docs::examples = "vector-logs"))]
    #[configurable(metadata(docs::required = true))]
    pub measurement: Option<String>,

    /// The endpoint to send data to.
    ///
    /// This should be a full HTTP URI, including the scheme, host, and port.
    #[configurable(metadata(docs::examples = "http://localhost:8086"))]
    pub endpoint: HttpEndpoint,

    /// The list of names of log fields that should be added as tags to each measurement.
    ///
    /// By default Vector adds `metric_type` as well as the configured `log_schema.host_key` and
    /// `log_schema.source_type_key` options.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "field1"))]
    #[configurable(metadata(docs::examples = "parent.child_field"))]
    pub tags: Vec<KeyString>,

    /// The InfluxDB API version to use.
    ///
    /// Omitting this option is deprecated and it will be required in a future release. When
    /// unset, the version is temporarily inferred from the configured settings.
    #[configurable(metadata(docs::examples = "2"))]
    #[configurable(metadata(docs::examples = "1"))]
    #[configurable(metadata(docs::minimal = true))]
    pub version: Option<InfluxDbVersion>,

    /// The name of the database to write into.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "vector-database"))]
    #[configurable(metadata(docs::relevant_when = "version = \"1\""))]
    #[configurable(metadata(docs::required_when = "version = \"1\""))]
    pub database: Option<String>,

    /// The consistency level to use for writes.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "any"))]
    #[configurable(metadata(docs::relevant_when = "version = \"1\""))]
    pub consistency: Option<String>,

    /// The target retention policy for writes.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "autogen"))]
    #[configurable(metadata(docs::relevant_when = "version = \"1\""))]
    pub retention_policy_name: Option<String>,

    /// The username to authenticate with.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "todd"))]
    #[configurable(metadata(docs::relevant_when = "version = \"1\""))]
    pub username: Option<String>,

    /// The password to authenticate with.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "${INFLUXDB_PASSWORD}"))]
    #[configurable(metadata(docs::relevant_when = "version = \"1\""))]
    pub password: Option<SensitiveString>,

    /// The name of the organization to write into.
    ///
    /// Only relevant when using InfluxDB v2.x and above.
    #[configurable(metadata(docs::examples = "my-org"))]
    #[configurable(metadata(docs::relevant_when = "version = \"2\""))]
    #[configurable(metadata(docs::required_when = "version = \"2\""))]
    #[configurable(metadata(docs::minimal = true))]
    pub org: Option<String>,

    /// The name of the bucket to write into.
    ///
    /// Only relevant when using InfluxDB v2.x and above.
    #[configurable(metadata(docs::examples = "vector-bucket"))]
    #[configurable(metadata(docs::relevant_when = "version = \"2\""))]
    #[configurable(metadata(docs::required_when = "version = \"2\""))]
    #[configurable(metadata(docs::minimal = true))]
    pub bucket: Option<String>,

    /// The [token][token_docs] to authenticate with.
    ///
    /// Only relevant when using InfluxDB v2.x and above.
    ///
    /// [token_docs]: https://v2.docs.influxdata.com/v2.0/security/tokens/
    #[configurable(metadata(docs::examples = "${INFLUXDB_TOKEN}"))]
    #[configurable(metadata(docs::relevant_when = "version = \"2\""))]
    #[configurable(metadata(docs::required_when = "version = \"2\""))]
    #[configurable(metadata(docs::minimal = true))]
    pub token: Option<SensitiveString>,

    #[serde(skip_serializing_if = "crate::serde::is_default", default)]
    pub encoding: Transformer,

    #[serde(default)]
    pub batch: BatchConfig<InfluxDbLogsDefaultBatchSettings>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    pub tls: Option<TlsConfig>,

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
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc! {r#"
            endpoint: http://localhost:8086/
            measurement: vector-logs
            tags: []
            version: "2"
            org: my-org
            bucket: my-bucket
            token: ${INFLUXDB_TOKEN}
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "influxdb_logs")]
impl SinkConfig for InfluxDbLogsConfig {
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

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct ValidatedInfluxDbLogs {
    measurement: String,
    tags: HashSet<KeyString>,
    batch: BatchSettings<Buffer>,
    // Omitted: the retained `uri` embeds the v1 password in its `p` query parameter.
    #[derivative(Debug = "ignore")]
    uri: Uri,
    token: SensitiveString,
    protocol_version: ProtocolVersion,
    host_key: Option<OwnedValuePath>,
    message_key: Option<OwnedValuePath>,
    source_type_key: Option<OwnedValuePath>,
}

#[async_trait::async_trait]
impl ValidatedSink for InfluxDbLogsConfig {
    type Validated = ValidatedInfluxDbLogs;

    fn validate(&self) -> crate::Result<ValidatedInfluxDbLogs> {
        let measurement = self.get_measurement()?;
        let tags: HashSet<KeyString> = self.tags.iter().cloned().collect();

        let batch = self.batch.into_batch_settings()?;

        let settings = influxdb_settings(self.settings()?);

        let uri = settings.write_uri(self.endpoint.clone())?;

        let token = settings.token();
        let protocol_version = settings.protocol_version();

        // Only the config-provided keys are retained here; the `log_schema()`
        // fallbacks are resolved in `build`, after the global log schema has
        // been initialized. Resolving them here would capture the built-in
        // defaults, since validation runs before `init_log_schema` at startup.
        let host_key = self.host_key.as_ref().and_then(|k| k.path.clone());
        let message_key = self.message_key.as_ref().and_then(|k| k.path.clone());
        let source_type_key = self.source_type_key.as_ref().and_then(|k| k.path.clone());

        Ok(ValidatedInfluxDbLogs {
            measurement,
            tags,
            batch,
            uri,
            token,
            protocol_version,
            host_key,
            message_key,
            source_type_key,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedInfluxDbLogs,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedInfluxDbLogs {
            measurement,
            tags,
            batch,
            uri,
            token,
            protocol_version,
            host_key,
            message_key,
            source_type_key,
        } = validated;

        let tls_settings = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;
        let healthcheck = self.healthcheck(client.clone())?;

        let request = self.request.into_settings();

        // Resolve the `log_schema()` fallbacks here, after the global log schema
        // has been initialized, so custom global log schema keys are honored.
        let host_key = host_key
            .clone()
            .or_else(|| log_schema().host_key().cloned())
            .expect("global log_schema.host_key to be valid path");
        let message_key = message_key
            .clone()
            .or_else(|| log_schema().message_key().cloned())
            .expect("global log_schema.message_key to be valid path");
        let source_type_key = source_type_key
            .clone()
            .or_else(|| log_schema().source_type_key().cloned())
            .expect("global log_schema.source_type_key to be valid path");

        let sink = InfluxDbLogsSink {
            uri: uri.clone(),
            token: token.inner().to_owned(),
            protocol_version: *protocol_version,
            measurement: measurement.clone(),
            tags: tags.clone(),
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
        .sink_map_err(|error| error!(message = "Fatal influxdb_logs sink error.", %error, internal_log_rate_limit = false));

        #[allow(deprecated)]
        Ok((VectorSink::from_event_sink(sink), healthcheck))
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
            if self.tags.contains(key.as_str()) {
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
    fn settings(&self) -> crate::Result<InfluxDbSettings> {
        let version = match self.version {
            Some(version) => {
                self.validate_version(version)?;
                version
            }
            None => {
                warn!(
                    "The `version` option is currently optional but will be required in a future release. \
                     Please set it to `1` or `2` to match your InfluxDB settings."
                );
                self.infer_version()?
            }
        };
        match version {
            InfluxDbVersion::V1 => Ok(InfluxDbSettings::V1(InfluxDb1Settings {
                database: self
                    .database
                    .clone()
                    .ok_or("the `database` option is required when using InfluxDB v1")?,
                consistency: self.consistency.clone(),
                retention_policy_name: self.retention_policy_name.clone(),
                username: self.username.clone(),
                password: self.password.clone(),
            })),
            InfluxDbVersion::V2 => Ok(InfluxDbSettings::V2(InfluxDb2Settings {
                org: self
                    .org
                    .clone()
                    .ok_or("the `org` option is required when using InfluxDB v2")?,
                bucket: self
                    .bucket
                    .clone()
                    .ok_or("the `bucket` option is required when using InfluxDB v2")?,
                token: self
                    .token
                    .clone()
                    .ok_or("the `token` option is required when using InfluxDB v2")?,
            })),
        }
    }

    const fn settings_present(&self) -> (bool, bool) {
        let has_v1 = self.database.is_some()
            || self.consistency.is_some()
            || self.retention_policy_name.is_some()
            || self.username.is_some()
            || self.password.is_some();
        let has_v2 = self.org.is_some() || self.bucket.is_some() || self.token.is_some();
        (has_v1, has_v2)
    }

    fn infer_version(&self) -> crate::Result<InfluxDbVersion> {
        let (has_v1, has_v2) = self.settings_present();
        match (has_v1, has_v2) {
            (true, true) => Err(
                "Unclear settings. Both InfluxDB v1 and v2 settings are configured; configure only one version."
                    .into(),
            ),
            (false, false) => Err("InfluxDB v1 or v2 should be configured as endpoint.".into()),
            (true, false) => Ok(InfluxDbVersion::V1),
            (false, true) => Ok(InfluxDbVersion::V2),
        }
    }

    /// Rejects settings that belong to the version that was not selected, so that
    /// an explicit `version` cannot silently ignore stale settings for the other version.
    fn validate_version(&self, version: InfluxDbVersion) -> crate::Result<()> {
        let (has_v1, has_v2) = self.settings_present();
        match version {
            InfluxDbVersion::V1 if has_v2 => Err(
                "InfluxDB v1 settings are configured, but v2 settings were also provided; configure only one version."
                    .into(),
            ),
            InfluxDbVersion::V2 if has_v1 => Err(
                "InfluxDB v2 settings are configured, but v1 settings were also provided; configure only one version."
                    .into(),
            ),
            _ => Ok(()),
        }
    }

    fn get_measurement(&self) -> Result<String, &'static str> {
        self.measurement
            .clone()
            .ok_or("The `measurement` option is required.")
    }

    fn healthcheck(&self, client: HttpClient) -> crate::Result<Healthcheck> {
        let config = self.clone();

        let healthcheck = healthcheck(config.endpoint.clone(), config.settings()?, client)?;

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
    use chrono::{Utc, offset::TimeZone};
    use futures::{StreamExt, channel::mpsc, stream};
    use http::{StatusCode, request::Parts};
    use indoc::indoc;
    use vector_lib::{
        event::{BatchNotifier, BatchStatus, Event, LogEvent},
        lookup::owned_value_path,
    };

    use super::*;
    use crate::{
        config::ValidatedSink,
        sinks::{
            influxdb::test_util::{assert_fields, split_line_protocol, ts},
            util::test::{build_test_server_status, load_sink},
        },
        test_util::{
            addr::next_addr,
            components::{
                COMPONENT_ERROR_TAGS, HTTP_SINK_TAGS, run_and_assert_sink_compliance,
                run_and_assert_sink_error,
            },
        },
    };

    type Receiver = mpsc::Receiver<(Parts, bytes::Bytes)>;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<InfluxDbLogsConfig>();
    }

    #[test]
    fn test_config_without_tags() {
        let config = indoc! {r#"
            measurement: "vector-logs"
            endpoint: "http://localhost:9999"
            version: "2"
            bucket: "my-bucket"
            org: "my-org"
            token: "my-token"
        "#};

        serde_yaml::from_str::<InfluxDbLogsConfig>(config).unwrap();
    }

    #[test]
    fn test_config_measurement_required() {
        let config = indoc! {r#"
            endpoint: "http://localhost:9999"
            version: "2"
            bucket: "my-bucket"
            org: "my-org"
            token: "my-token"
        "#};

        let sink_config = serde_yaml::from_str::<InfluxDbLogsConfig>(config).unwrap();
        assert_eq!(
            Err("The `measurement` option is required."),
            sink_config.get_measurement()
        );
    }

    #[test]
    fn prepares_valid_config() {
        let config = InfluxDbLogsConfig {
            measurement: Some("vector".to_string()),
            endpoint: HttpEndpoint::parse("http://localhost:9999").unwrap(),
            org: Some("my-org".to_string()),
            bucket: Some("my-bucket".to_string()),
            token: Some("my-token".to_string().into()),
            tags: vec![],
            version: Some(InfluxDbVersion::V2),
            database: None,
            consistency: None,
            retention_policy_name: None,
            username: None,
            password: None,
            encoding: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            tls: None,
            acknowledgements: Default::default(),
            host_key: None,
            message_key: None,
            source_type_key: None,
        };

        let validated = config.validate().expect("preparation should succeed");
        assert_eq!(validated.measurement, "vector");
        assert!(matches!(validated.protocol_version, ProtocolVersion::V2));
        assert_eq!(
            validated.uri.to_string(),
            "http://localhost:9999/api/v2/write?org=my-org&bucket=my-bucket&precision=ns"
        );
    }

    #[test]
    fn validate_retains_config_keys_without_log_schema_fallback() {
        let config = InfluxDbLogsConfig {
            measurement: Some("vector".to_string()),
            endpoint: HttpEndpoint::parse("http://localhost:9999").unwrap(),
            org: Some("my-org".to_string()),
            bucket: Some("my-bucket".to_string()),
            token: Some("my-token".to_string().into()),
            host_key: Some(OptionalValuePath::new("custom_host")),
            tags: vec![],
            version: Some(InfluxDbVersion::V2),
            database: None,
            consistency: None,
            retention_policy_name: None,
            username: None,
            password: None,
            encoding: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            tls: None,
            acknowledgements: Default::default(),
            message_key: None,
            source_type_key: None,
        };

        let validated = config.validate().expect("validation should succeed");
        // Config-provided keys are retained...
        assert_eq!(validated.host_key, Some(owned_value_path!("custom_host")));
        // ...but unset keys stay unset: `validate` must not resolve the global
        // `log_schema()` defaults, which aren't initialized yet at validation
        // time in the startup path. The fallbacks are resolved in `build`.
        assert_eq!(validated.message_key, None);
        assert_eq!(validated.source_type_key, None);
    }

    #[test]
    fn test_encode_event_apply_rules() {
        let mut event = Event::Log(LogEvent::from("hello"));
        event
            .as_mut_log()
            .insert(event_path!("host"), "aws.cloud.eur");
        event.as_mut_log().insert(event_path!("timestamp"), ts());

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
        event
            .as_mut_log()
            .insert(event_path!("host"), "aws.cloud.eur");
        event
            .as_mut_log()
            .insert(event_path!("source_type"), "file");

        event.as_mut_log().insert(event_path!("int"), 4i32);
        event.as_mut_log().insert(event_path!("float"), 5.5);
        event.as_mut_log().insert(event_path!("bool"), true);
        event
            .as_mut_log()
            .insert(event_path!("string"), "thisisastring");
        event.as_mut_log().insert(event_path!("timestamp"), ts());

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
        event
            .as_mut_log()
            .insert(event_path!("host"), "aws.cloud.eur");
        event
            .as_mut_log()
            .insert(event_path!("source_type"), "file");

        event.as_mut_log().insert(event_path!("int"), 4i32);
        event.as_mut_log().insert(event_path!("float"), 5.5);
        event.as_mut_log().insert(event_path!("bool"), true);
        event
            .as_mut_log()
            .insert(event_path!("string"), "thisisastring");
        event.as_mut_log().insert(event_path!("timestamp"), ts());

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

        event.as_mut_log().insert(event_path!("value"), 100);
        event.as_mut_log().insert(event_path!("timestamp"), ts());

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

        event.insert(event_path!("a"), 1);
        event.insert(event_path!("nested", "field"), "2");
        event.insert(event_path!("nested", "bool"), true);
        event.insert(event_path!("nested", "array", 0isize), "example-value");
        event.insert(event_path!("nested", "array", 2isize), "another-value");
        event.insert(event_path!("nested", "array", 3isize), 15);

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
        event
            .as_mut_log()
            .insert(event_path!("source_type"), "file");

        event.as_mut_log().insert(event_path!("as_a_tag"), 10);
        event.as_mut_log().insert(event_path!("timestamp"), ts());

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
            indoc! {r#"
            version = "1"
            database = "my-database"
        "#},
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
            indoc! {r#"
            version = "1"
            database = "my-database"
        "#},
            StatusCode::BAD_REQUEST,
            BatchStatus::Rejected,
        )
        .await;
    }

    #[tokio::test]
    async fn smoke_v2() {
        let rx = smoke_test(
            indoc! {r#"
            version = "2"
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
            version = "2"
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
        _ = SinkConfig::build(&config, cx.clone()).await.unwrap();

        let (_guard, addr) = next_addr();
        // Swap out the host so we can force send it
        // to our local server
        let host = format!("http://{addr}");
        config.endpoint = HttpEndpoint::parse(&host).unwrap();

        let (sink, _) = SinkConfig::build(&config, cx).await.unwrap();

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
            event.insert(event_path!(format!("key{i}").as_str()), format!("value{i}"));

            let timestamp = Utc
                .with_ymd_and_hms(1970, 1, 1, 0, 0, (i as u32) + 1)
                .single()
                .expect("invalid timestamp");
            event.insert(event_path!("timestamp"), timestamp);
            event.insert(event_path!("source_type"), "file");

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
                &*format!("key{i}=\"value{i}\""),
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
    use vector_lib::{
        codecs::BytesDeserializerConfig,
        config::{LegacyKey, LogNamespace},
        event::{BatchNotifier, BatchStatus, Event, LogEvent},
        lookup::{owned_value_path, path},
    };
    use vrl::value;

    use super::*;
    use crate::{
        config::SinkContext,
        sinks::influxdb::{
            InfluxDbVersion,
            logs::InfluxDbLogsConfig,
            test_util::{BUCKET, ORG, TOKEN, address_v2, onboarding_v2},
        },
        test_util::components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
    };

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
            measurement: Some(measure.clone()),
            endpoint: HttpEndpoint::parse(&endpoint).unwrap(),
            tags: Default::default(),
            version: Some(InfluxDbVersion::V2),
            database: None,
            consistency: None,
            retention_policy_name: None,
            username: None,
            password: None,
            org: Some(ORG.to_string()),
            bucket: Some(BUCKET.to_string()),
            token: Some(TOKEN.to_string().into()),
            encoding: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            tls: None,
            acknowledgements: Default::default(),
            host_key: None,
            message_key: None,
            source_type_key: None,
        };

        let (sink, _) = SinkConfig::build(&config, cx).await.unwrap();

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();

        let mut event1 = LogEvent::from("message_1").with_batch_notifier(&batch);
        event1.insert(event_path!("host"), "aws.cloud.eur");
        event1.insert(event_path!("source_type"), "file");

        let mut event2 = LogEvent::from("message_2").with_batch_notifier(&batch);
        event2.insert(event_path!("host"), "aws.cloud.eur");
        event2.insert(event_path!("source_type"), "file");

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
            .post(format!("{endpoint}/api/v2/query?org=my-org"))
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
