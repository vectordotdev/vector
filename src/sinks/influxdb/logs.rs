use crate::dns::Resolver;
use crate::event::Value;
use crate::sinks::influxdb::{
    encode_namespace, encode_timestamp, healthcheck, influx_line_protocol, influxdb_settings,
    Field, InfluxDB1Settings, InfluxDB2Settings, ProtocolVersion,
};
use crate::sinks::util::encoding::EncodingConfigWithDefault;
use crate::sinks::util::http::{BatchedHttpSink, HttpSink};
use crate::sinks::util::{service2::TowerRequestConfig, BatchBytesConfig, Buffer, Compression};
use crate::sinks::Healthcheck;
use crate::{
    event::{log_schema, Event},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures01::Sink;
use http::{Request, Uri};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

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

#[derive(Debug)]
struct InfluxDBLogsSink {
    uri: Uri,
    token: String,
    protocol_version: ProtocolVersion,
    namespace: String,
    tags: HashSet<String>,
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
        // let mut config = self.clone();
        let mut tags: HashSet<String> = self.tags.clone().into_iter().collect();
        tags.insert(log_schema().host_key().to_string());
        tags.insert(log_schema().source_type_key().to_string());

        let healthcheck = self.healthcheck(cx.resolver())?;

        let batch = self.batch.unwrap_or(bytesize::mib(1u64), 1);
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);

        let settings = influxdb_settings(
            self.influxdb1_settings.clone(),
            self.influxdb2_settings.clone(),
        )
        .unwrap();

        let endpoint = self.endpoint.clone();
        let uri = settings.write_uri(endpoint).unwrap();

        let token = settings.token();
        let protocol_version = settings.protocol_version();
        let namespace = self.namespace.clone();

        let sink = InfluxDBLogsSink {
            uri,
            token,
            protocol_version,
            namespace,
            tags,
        };

        let sink = BatchedHttpSink::new(
            sink,
            Buffer::new(Compression::None),
            request,
            batch,
            None,
            &cx,
        )
        .sink_map_err(|e| error!("Fatal influxdb_logs sink error: {}", e));

        Ok((Box::new(sink), Box::new(healthcheck)))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "influxdb_logs"
    }
}

#[async_trait::async_trait]
impl HttpSink for InfluxDBLogsSink {
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode_event(&self, event: Event) -> Option<Self::Input> {
        let mut output = String::new();
        let mut event = event.into_log();

        // Measurement
        let measurement = encode_namespace(&self.namespace, &"vector");

        // Timestamp
        let timestamp = encode_timestamp(match event.remove(log_schema().timestamp_key()) {
            Some(Value::Timestamp(ts)) => Some(ts),
            _ => None,
        });

        // Tags + Fields
        let mut tags: BTreeMap<String, String> = BTreeMap::new();
        let mut fields: HashMap<String, Field> = HashMap::new();
        event.all_fields().for_each(|(key, value)| {
            if self.tags.contains(&key) {
                tags.insert(key, value.to_string_lossy());
            } else {
                fields.insert(key, value.to_field());
            }
        });

        influx_line_protocol(
            self.protocol_version,
            measurement,
            "logs",
            Some(tags),
            Some(fields),
            timestamp,
            &mut output,
        );

        Some(output.into_bytes())
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
        Request::post(&self.uri)
            .header("Content-Type", "text/plain")
            .header("Authorization", format!("Token {}", &self.token))
            .body(events)
            .map_err(Into::into)
    }
}

impl InfluxDBLogsConfig {
    fn healthcheck(&self, resolver: Resolver) -> crate::Result<Healthcheck> {
        let config = self.clone();

        let healthcheck = healthcheck(
            config.endpoint,
            config.influxdb1_settings,
            config.influxdb2_settings,
            resolver,
        )?;

        Ok(Box::new(healthcheck))
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

        toml::from_str::<InfluxDBLogsConfig>(&config).unwrap();
    }

    #[test]
    fn test_encode_event_v1() {
        let mut event = Event::from("hello");
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
            "ns",
            ["source_type", "host"].to_vec(),
        );

        let bytes = sink.encode_event(event).unwrap();
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
    fn test_encode_event() {
        let mut event = Event::from("hello");
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
            "ns",
            ["source_type", "host"].to_vec(),
        );

        let bytes = sink.encode_event(event).unwrap();
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

        let sink = create_sink(
            "http://localhost:9999",
            "my-token",
            ProtocolVersion::V2,
            "ns",
            [].to_vec(),
        );

        let bytes = sink.encode_event(event).unwrap();
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
    fn test_encode_nested_fields() {
        let mut event = Event::new_empty_log();

        event.as_mut_log().insert("a", 1);
        event.as_mut_log().insert("nested.field", "2");
        event.as_mut_log().insert("nested.bool", true);
        event
            .as_mut_log()
            .insert("nested.array[0]", "example-value");
        event
            .as_mut_log()
            .insert("nested.array[2]", "another-value");
        event.as_mut_log().insert("nested.array[3]", 15);

        let sink = create_sink(
            "http://localhost:9999",
            "my-token",
            ProtocolVersion::V2,
            "ns",
            [].to_vec(),
        );

        let bytes = sink.encode_event(event).unwrap();
        let string = std::str::from_utf8(&bytes).unwrap();

        let line_protocol = split_line_protocol(&string);
        assert_eq!("ns.vector", line_protocol.0);
        assert_eq!("metric_type=logs", line_protocol.1);
        assert_fields(
            line_protocol.2.to_string(),
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
        let mut event = Event::from("hello");
        event.as_mut_log().insert("source_type", "file");

        event.as_mut_log().insert("as_a_tag", 10);
        event.as_mut_log().insert("timestamp", ts());

        let sink = create_sink(
            "http://localhost:9999",
            "my-token",
            ProtocolVersion::V2,
            "ns",
            ["as_a_tag", "not_exists_field", "source_type"].to_vec(),
        );

        let bytes = sink.encode_event(event).unwrap();
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

        let (rx, _trigger, server) = build_test_server(addr, &mut rt);
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
            event.as_mut_log().insert("source_type", "file");

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

        let (rx, _trigger, server) = build_test_server(addr, &mut rt);
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
            event.as_mut_log().insert("source_type", "file");

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
        assert_eq!("metric_type=logs,source_type=file", line_protocol.1);
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

    fn create_sink(
        uri: &str,
        token: &str,
        protocol_version: ProtocolVersion,
        namespace: &str,
        tags: Vec<&str>,
    ) -> InfluxDBLogsSink {
        let uri = uri.parse::<Uri>().unwrap();
        let token = token.to_string();
        let namespace = namespace.to_string();
        let tags: HashSet<String> = tags.into_iter().map(|tag| tag.to_string()).collect();
        let sink = InfluxDBLogsSink {
            uri,
            token,
            protocol_version,
            namespace,
            tags,
        };
        sink
    }
}

#[cfg(feature = "influxdb-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::sinks::influxdb::logs::InfluxDBLogsConfig;
    use crate::sinks::influxdb::test_util::{onboarding_v2, BUCKET, ORG, TOKEN};
    use crate::sinks::influxdb::InfluxDB2Settings;
    use crate::test_util::runtime;
    use crate::topology::SinkContext;
    use chrono::Utc;
    use futures01::Sink;

    #[test]
    fn influxdb2_logs_put_data() {
        onboarding_v2();

        let ns = format!("ns-{}", Utc::now().timestamp_nanos());

        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let config = InfluxDBLogsConfig {
            namespace: ns.clone(),
            endpoint: "http://localhost:9999".to_string(),
            tags: Default::default(),
            influxdb1_settings: None,
            influxdb2_settings: Some(InfluxDB2Settings {
                org: ORG.to_string(),
                bucket: BUCKET.to_string(),
                token: TOKEN.to_string(),
            }),
            encoding: Default::default(),
            batch: Default::default(),
            request: Default::default(),
        };

        let (sink, _) = config.build(cx).unwrap();

        let mut events = Vec::new();

        let mut event1 = Event::from("message_1");
        event1.as_mut_log().insert("host", "aws.cloud.eur");
        event1.as_mut_log().insert("source_type", "file");

        let mut event2 = Event::from("message_2");
        event2.as_mut_log().insert("host", "aws.cloud.eur");
        event2.as_mut_log().insert("source_type", "file");

        events.push(event1);
        events.push(event2);

        let pump = sink.send_all(futures01::stream::iter_ok(events));
        let _ = rt.block_on(pump).unwrap();

        let mut body = std::collections::HashMap::new();
        body.insert("query", format!("from(bucket:\"my-bucket\") |> range(start: 0) |> filter(fn: (r) => r._measurement == \"{}.vector\")", ns.clone()));
        body.insert("type", "flux".to_owned());

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let mut res = client
            .post("http://localhost:9999/api/v2/query?org=my-org")
            .json(&body)
            .header("accept", "application/json")
            .header("Authorization", "Token my-token")
            .send()
            .unwrap();
        let result = res.text();
        let string = result.unwrap();

        let lines = string.split("\n").collect::<Vec<&str>>();
        let header = lines[0].split(",").collect::<Vec<&str>>();
        let record1 = lines[1].split(",").collect::<Vec<&str>>();
        let record2 = lines[2].split(",").collect::<Vec<&str>>();

        // measurement
        assert_eq!(
            record1[header
                .iter()
                .position(|&r| r.trim() == "_measurement")
                .unwrap()]
            .trim(),
            format!("{}.vector", ns.clone())
        );
        assert_eq!(
            record2[header
                .iter()
                .position(|&r| r.trim() == "_measurement")
                .unwrap()]
            .trim(),
            format!("{}.vector", ns.clone())
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
            record1[header.iter().position(|&r| r.trim() == "host").unwrap()].trim(),
            "aws.cloud.eur"
        );
        assert_eq!(
            record2[header.iter().position(|&r| r.trim() == "host").unwrap()].trim(),
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
            record1[header.iter().position(|&r| r.trim() == "_value").unwrap()].trim(),
            "message_1"
        );
        assert_eq!(
            record2[header.iter().position(|&r| r.trim() == "_value").unwrap()].trim(),
            "message_2"
        );
    }
}
