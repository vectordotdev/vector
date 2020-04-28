use crate::event::Value;
use crate::sinks::influxdb::{
    encode_namespace, encode_timestamp, healthcheck, influx_line_protocol, influxdb_settings,
    Field, InfluxDB1Settings, InfluxDB2Settings,
};
use crate::sinks::util::encoding::EncodingConfigWithDefault;
use crate::sinks::util::http::{BatchedHttpSink, HttpSink};
use crate::sinks::util::{BatchBytesConfig, Buffer, TowerRequestConfig};
use crate::{
    event::{log_schema, Event},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures01::Sink;
use http::{Method, Request};
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

        Some(output.into_bytes())
    }

    fn build_request(&self, events: Self::Output) -> http::Request<Vec<u8>> {
        let settings = influxdb_settings(
            self.influxdb1_settings.clone(),
            self.influxdb2_settings.clone(),
        )
        .unwrap();

        let endpoint = self.endpoint.clone();
        let token = settings.token();

        let uri = settings.write_uri(endpoint).unwrap();

        let mut builder = Request::builder();
        builder.method(Method::POST);
        builder.uri(uri.clone());

        builder.header("Content-Type", "text/plain");
        builder.header("Authorization", format!("Token {}", token));
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
    use crate::event::Event;
    use crate::sinks::influxdb::test_util::{assert_fields, split_line_protocol, ts};
    use crate::sinks::util::http::HttpSink;
    use crate::sinks::util::test::build_test_server;
    use crate::test_util;
    use chrono::offset::TimeZone;
    use chrono::Utc;
    use futures01::{Sink, Stream};

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

        assert_eq!("1542182950000000011\n", line_protocol.3);
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

        assert_eq!("1542182950000000011\n", line_protocol.3);
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

        assert_eq!("1542182950000000011\n", line_protocol.3);
    }

    #[test]
    fn smoke_v1() {
        let (mut config, cx, mut rt) = crate::sinks::util::test::load_sink::<InfluxDBLogsConfig>(
            r#"
            namespace = "ns"
            endpoint = "http://localhost:9999"
            database = "my-database"
        "#,
        )
        .unwrap();

        // Make sure we can build the config
        let _ = config.build(cx.clone()).unwrap();

        let addr = test_util::next_addr();
        // Swap out the host so we can force send it
        // to our local server
        let host = format!("http://{}", addr);
        config.endpoint = host;

        let (sink, _) = config.build(cx).unwrap();

        let (rx, _trigger, server) = build_test_server(&addr);
        rt.spawn(server);

        let lines = std::iter::repeat(())
            .map(move |_| "message_value")
            .take(5)
            .collect::<Vec<_>>();
        let mut events = Vec::new();

        // Create 5 events with custom field
        for (i, line) in lines.iter().enumerate() {
            let mut event = Event::from(line.to_string());
            event
                .as_mut_log()
                .insert(format!("key{}", i), format!("value{}", i));

            let timestamp = Utc.ymd(1970, 01, 01).and_hms_nano(0, 0, (i as u32) + 1, 0);
            event.as_mut_log().insert("timestamp", timestamp);

            events.push(event);
        }

        let pump = sink.send_all(futures01::stream::iter_ok(events));
        let _ = rt.block_on(pump).unwrap();

        let output = rx.take(1).wait().collect::<Result<Vec<_>, _>>().unwrap();

        let request = &output[0].0;
        let query = request.uri.query().unwrap();
        assert!(query.contains("db=my-database"));
        assert!(query.contains("precision=ns"));

        let body = std::str::from_utf8(&output[0].1[..]).unwrap();
        let mut lines = body.lines();

        assert_eq!(5, lines.clone().count());
        assert_line_protocol(0, lines.next());
    }

    #[test]
    fn smoke_v2() {
        let (mut config, cx, mut rt) = crate::sinks::util::test::load_sink::<InfluxDBLogsConfig>(
            r#"
            namespace = "ns"
            endpoint = "http://localhost:9999"
            bucket = "my-bucket"
            org = "my-org"
            token = "my-token"
        "#,
        )
        .unwrap();

        // Make sure we can build the config
        let _ = config.build(cx.clone()).unwrap();

        let addr = test_util::next_addr();
        // Swap out the host so we can force send it
        // to our local server
        let host = format!("http://{}", addr);
        config.endpoint = host;

        let (sink, _) = config.build(cx).unwrap();

        let (rx, _trigger, server) = build_test_server(&addr);
        rt.spawn(server);

        let lines = std::iter::repeat(())
            .map(move |_| "message_value")
            .take(5)
            .collect::<Vec<_>>();
        let mut events = Vec::new();

        // Create 5 events with custom field
        for (i, line) in lines.iter().enumerate() {
            let mut event = Event::from(line.to_string());
            event
                .as_mut_log()
                .insert(format!("key{}", i), format!("value{}", i));

            let timestamp = Utc.ymd(1970, 01, 01).and_hms_nano(0, 0, (i as u32) + 1, 0);
            event.as_mut_log().insert("timestamp", timestamp);

            events.push(event);
        }

        let pump = sink.send_all(futures01::stream::iter_ok(events));
        let _ = rt.block_on(pump).unwrap();

        let output = rx.take(1).wait().collect::<Result<Vec<_>, _>>().unwrap();

        let request = &output[0].0;
        let query = request.uri.query().unwrap();
        assert!(query.contains("org=my-org"));
        assert!(query.contains("bucket=my-bucket"));
        assert!(query.contains("precision=ns"));

        let body = std::str::from_utf8(&output[0].1[..]).unwrap();
        let mut lines = body.lines();

        assert_eq!(5, lines.clone().count());
        assert_line_protocol(0, lines.next());
    }

    fn assert_line_protocol(i: i64, value: Option<&str>) {
        //ns.vector,metric_type=logs key0="value0",message="message_value" 1000000000
        let line_protocol = split_line_protocol(value.unwrap());
        assert_eq!("ns.vector", line_protocol.0);
        assert_eq!("metric_type=logs", line_protocol.1);
        assert_fields(
            line_protocol.2.to_string(),
            [
                &*format!("key{}=\"value{}\"", i, i),
                "message=\"message_value\"",
            ]
            .to_vec(),
        );

        assert_eq!(format!("{}", (i + 1) * 1000000000), line_protocol.3);
    }
}
