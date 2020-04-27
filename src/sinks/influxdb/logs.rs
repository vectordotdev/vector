use crate::event::Value;
use crate::sinks::influxdb::{
    encode_namespace, encode_timestamp, healthcheck, influx_line_protocol, Field,
    InfluxDB1Settings, InfluxDB2Settings,
};
use crate::sinks::util::encoding::EncodingConfigWithDefault;
use crate::sinks::util::http::{BatchedHttpSink, HttpSink};
use crate::sinks::util::{BatchBytesConfig, Buffer, TowerRequestConfig};
use crate::{
    event::{log_schema, Event},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures01::Sink;
use http::Request;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct InfluxDBLogsConfig {
    pub namespace: String,
    pub endpoint: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub influxdb1_settings: Option<InfluxDB1Settings>,
    #[serde(flatten)]
    pub influxdb2_settings: Option<InfluxDB2Settings>,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    pub batch: BatchBytesConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        retry_attempts: Some(5),
        ..Default::default()
    };
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

inventory::submit! {
    SinkDescription::new_without_default::<InfluxDBLogsConfig>("influxdb_logs")
}

#[typetag::serde(name = "influxdb_logs")]
impl SinkConfig for InfluxDBLogsConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let healthcheck = healthcheck(
            self.clone().endpoint,
            self.clone().influxdb1_settings,
            self.clone().influxdb2_settings,
            cx.resolver(),
        )?;

        let batch = self.batch.unwrap_or(bytesize::mib(1u64), 1);
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);

        let sink =
            BatchedHttpSink::new(self.clone(), Buffer::new(false), request, batch, None, &cx)
                .sink_map_err(|e| error!("Fatal influxdb_logs sink error: {}", e));

        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "influxdb_logs"
    }
}

impl HttpSink for InfluxDBLogsConfig {
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode_event(&self, event: Event) -> Option<Self::Input> {
        let mut output = String::new();
        let mut event = event.into_log();

        let tag_keys: Vec<String> = [
            vec!["host".to_owned(), "source_type".to_owned()],
            self.tags.clone(),
        ]
        .concat();

        // Measurement
        let measurement = encode_namespace(&self.namespace, &"vector");

        // Timestamp
        let timestamp =
            if let Some(Value::Timestamp(ts)) = event.remove(log_schema().timestamp_key()) {
                encode_timestamp(Some(ts))
            } else {
                encode_timestamp(None)
            };

        // Tags
        let tags: BTreeMap<String, String> = event
            .all_fields()
            .filter(|(key, _)| tag_keys.contains(key))
            .map(|(key, value)| (key, value.to_string_lossy()))
            .collect();

        // Fields
        let fields: HashMap<String, Field> = event
            .all_fields()
            .filter(|(key, _)| !tag_keys.contains(key))
            .map(|(key, value)| (key, value.to_field()))
            .collect();

        influx_line_protocol(
            measurement,
            "logs",
            Some(tags),
            Some(fields),
            timestamp,
            &mut output,
        );
        // remove last '\n'
        output.pop();

        Some(output.into_bytes())
    }

    fn build_request(&self, events: Self::Output) -> http::Request<Vec<u8>> {
        let mut builder = Request::builder();

        builder.body(events).unwrap()
    }
}

impl Value {
    pub fn to_field(&self) -> Field {
        match self {
            Value::Integer(num) => Field::Int(*num),
            Value::Float(num) => Field::Float(*num),
            Value::Boolean(b) => Field::Bool(*b),
            _ => Field::String(self.to_string_lossy()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{self, Event};
    use crate::sinks::influxdb::test_util::{assert_fields, split_line_protocol, ts};
    use crate::sinks::util::http::HttpSink;

    #[test]
    fn test_config_without_tags() {
        let config = r#"
            namespace = "vector-logs"
            endpoint = "http://localhost:9999"
            bucket = "my-bucket"
            org = "my-org"
            token = "my-token"
        "#;

        let _: InfluxDBLogsConfig = toml::from_str(&config).unwrap();
    }

    #[test]
    fn test_encode_event() {
        let mut event = Event::from("hello");
        event.as_mut_log().insert("host", "aws.cloud.eur");
        event.as_mut_log().insert("source_type", "file");

        event.as_mut_log().insert("int", 4);
        event.as_mut_log().insert("float", 5.5);
        event.as_mut_log().insert("bool", true);
        event.as_mut_log().insert("string", "thisisastring");
        event.as_mut_log().insert("timestamp", ts());

        let (config, _, _) = crate::sinks::util::test::load_sink::<InfluxDBLogsConfig>(
            r#"
            namespace = "ns"
            endpoint = "http://localhost:9999"
            bucket = "my-bucket"
            org = "my-org"
            token = "my-token"
        "#,
        )
        .unwrap();

        let bytes = config.encode_event(event.clone()).unwrap();
        let string = std::str::from_utf8(&bytes).unwrap();

        let line_protocol = split_line_protocol(&string);
        assert_eq!("ns.vector", line_protocol.0);
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

        assert_eq!("1542182950000000011", line_protocol.3);
    }

    #[test]
    fn test_encode_event_without_tags() {
        let mut event = Event::from("hello");

        event.as_mut_log().insert("value", 100);
        event.as_mut_log().insert("timestamp", ts());

        let (config, _, _) = crate::sinks::util::test::load_sink::<InfluxDBLogsConfig>(
            r#"
            namespace = "ns"
            endpoint = "http://localhost:9999"
            bucket = "my-bucket"
            org = "my-org"
            token = "my-token"
        "#,
        )
        .unwrap();

        let bytes = config.encode_event(event.clone()).unwrap();
        let string = std::str::from_utf8(&bytes).unwrap();

        let line_protocol = split_line_protocol(&string);
        assert_eq!("ns.vector", line_protocol.0);
        assert_eq!("metric_type=logs", line_protocol.1);
        assert_fields(
            line_protocol.2.to_string(),
            ["value=100i", "message=\"hello\""].to_vec(),
        );

        assert_eq!("1542182950000000011", line_protocol.3);
    }

    #[test]
    fn test_add_tag() {
        let mut event = Event::from("hello");
        event.as_mut_log().insert("source_type", "file");

        event.as_mut_log().insert("as_a_tag", 10);
        event.as_mut_log().insert("timestamp", ts());

        let (config, _, _) = crate::sinks::util::test::load_sink::<InfluxDBLogsConfig>(
            r#"
            namespace = "ns"
            endpoint = "http://localhost:9999"
            bucket = "my-bucket"
            org = "my-org"
            token = "my-token"
            tags = ["as_a_tag", "not_exists_field"]
        "#,
        )
        .unwrap();

        let bytes = config.encode_event(event.clone()).unwrap();
        let string = std::str::from_utf8(&bytes).unwrap();

        let line_protocol = split_line_protocol(&string);
        assert_eq!("ns.vector", line_protocol.0);
        assert_eq!(
            "as_a_tag=10,metric_type=logs,source_type=file",
            line_protocol.1
        );
        assert_fields(line_protocol.2.to_string(), ["message=\"hello\""].to_vec());

        assert_eq!("1542182950000000011", line_protocol.3);
    }
}
