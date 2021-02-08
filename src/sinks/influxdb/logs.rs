use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::{Event, Value},
    http::HttpClient,
    sinks::{
        influxdb::{
            encode_timestamp, healthcheck, influx_line_protocol, influxdb_settings, Field,
            InfluxDB1Settings, InfluxDB2Settings, ProtocolVersion,
        },
        util::{
            encode_namespace,
            encoding::{EncodingConfig, EncodingConfigWithDefault, EncodingConfiguration},
            http::{BatchedHttpSink, HttpSink},
            BatchConfig, BatchSettings, Buffer, Compression, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    tls::{TlsOptions, TlsSettings},
};
use futures::SinkExt;
use http::{Request, Uri};
use indoc::indoc;
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
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

#[derive(Debug)]
struct InfluxDBLogsSink {
    uri: Uri,
    token: String,
    protocol_version: ProtocolVersion,
    namespace: String,
    tags: HashSet<String>,
    encoding: EncodingConfig<Encoding>,
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
    SinkDescription::new::<InfluxDBLogsConfig>("influxdb_logs")
}

impl GenerateConfig for InfluxDBLogsConfig {
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
impl SinkConfig for InfluxDBLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let mut tags: HashSet<String> = self.tags.clone().into_iter().collect();
        tags.insert(log_schema().host_key().to_string());
        tags.insert(log_schema().source_type_key().to_string());

        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings)?;
        let healthcheck = self.healthcheck(client.clone())?;

        let batch = BatchSettings::default()
            .bytes(bytesize::mib(1u64))
            .timeout(1)
            .parse_config(self.batch)?;
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
            encoding: self.encoding.clone().into(),
        };

        let sink = BatchedHttpSink::new(
            sink,
            Buffer::new(batch.size, Compression::None),
            request,
            batch.timeout,
            client,
            cx.acker(),
        )
        .sink_map_err(|error| error!(message = "Fatal influxdb_logs sink error.", %error));

        Ok((VectorSink::Sink(Box::new(sink)), healthcheck))
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

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.encoding.apply_rules(&mut event);
        let mut event = event.into_log();

        // Measurement
        let name = "vector";
        let measurement = encode_namespace(
            Some(self.namespace.as_str()).filter(|namespace| !namespace.is_empty()),
            '.',
            name,
        );

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

        let mut output = String::new();
        if let Err(error) = influx_line_protocol(
            self.protocol_version,
            measurement,
            "logs",
            Some(tags),
            Some(fields),
            timestamp,
            &mut output,
        ) {
            warn!(message = "Failed to encode event; dropping event.", %error, internal_log_rate_secs = 30);
            return None;
        };

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

impl Value {
    fn to_field(&self) -> Field {
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
    use crate::{
        event::Event,
        sinks::influxdb::test_util::{assert_fields, split_line_protocol, ts},
        sinks::util::{
            http::HttpSink,
            test::{build_test_server, load_sink},
        },
        test_util::next_addr,
    };
    use chrono::{offset::TimeZone, Utc};
    use futures::{stream, StreamExt};
    use indoc::indoc;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<InfluxDBLogsConfig>();
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

        toml::from_str::<InfluxDBLogsConfig>(&config).unwrap();
    }

    #[test]
    fn test_encode_event_apply_rules() {
        let mut event = Event::from("hello");
        event.as_mut_log().insert("host", "aws.cloud.eur");
        event.as_mut_log().insert("timestamp", ts());

        let mut sink = create_sink(
            "http://localhost:9999",
            "my-token",
            ProtocolVersion::V1,
            "ns",
            vec![],
        );
        sink.encoding.except_fields = Some(vec!["host".into()]);

        let bytes = sink.encode_event(event).unwrap();
        let string = std::str::from_utf8(&bytes).unwrap();

        let line_protocol = split_line_protocol(&string);
        assert_eq!("ns.vector", line_protocol.0);
        assert_eq!("metric_type=logs", line_protocol.1);
        assert_fields(line_protocol.2.to_string(), ["message=\"hello\""].to_vec());
        assert_eq!("1542182950000000011\n", line_protocol.3);
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

    #[tokio::test]
    async fn smoke_v1() {
        let (mut config, cx) = load_sink::<InfluxDBLogsConfig>(indoc! {r#"
            namespace = "ns"
            endpoint = "http://localhost:9999"
            database = "my-database"
        "#})
        .unwrap();

        // Make sure we can build the config
        let _ = config.build(cx.clone()).await.unwrap();

        let addr = next_addr();
        // Swap out the host so we can force send it
        // to our local server
        let host = format!("http://{}", addr);
        config.endpoint = host;

        let (sink, _) = config.build(cx).await.unwrap();

        let (mut rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

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

            let timestamp = Utc.ymd(1970, 1, 1).and_hms_nano(0, 0, (i as u32) + 1, 0);
            event.as_mut_log().insert("timestamp", timestamp);
            event.as_mut_log().insert("source_type", "file");

            events.push(event);
        }

        sink.run(stream::iter(events)).await.unwrap();

        let output = rx.next().await.unwrap();

        let request = &output.0;
        let query = request.uri.query().unwrap();
        assert!(query.contains("db=my-database"));
        assert!(query.contains("precision=ns"));

        let body = std::str::from_utf8(&output.1[..]).unwrap();
        let mut lines = body.lines();

        assert_eq!(5, lines.clone().count());
        assert_line_protocol(0, lines.next());
    }

    #[tokio::test]
    async fn smoke_v2() {
        let (mut config, cx) = load_sink::<InfluxDBLogsConfig>(indoc! {r#"
            namespace = "ns"
            endpoint = "http://localhost:9999"
            bucket = "my-bucket"
            org = "my-org"
            token = "my-token"
        "#})
        .unwrap();

        // Make sure we can build the config
        let _ = config.build(cx.clone()).await.unwrap();

        let addr = next_addr();
        // Swap out the host so we can force send it
        // to our local server
        let host = format!("http://{}", addr);
        config.endpoint = host;

        let (sink, _) = config.build(cx).await.unwrap();

        let (mut rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

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

            let timestamp = Utc.ymd(1970, 1, 1).and_hms_nano(0, 0, (i as u32) + 1, 0);
            event.as_mut_log().insert("timestamp", timestamp);
            event.as_mut_log().insert("source_type", "file");

            events.push(event);
        }

        sink.run(stream::iter(events)).await.unwrap();

        let output = rx.next().await.unwrap();

        let request = &output.0;
        let query = request.uri.query().unwrap();
        assert!(query.contains("org=my-org"));
        assert!(query.contains("bucket=my-bucket"));
        assert!(query.contains("precision=ns"));

        let body = std::str::from_utf8(&output.1[..]).unwrap();
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
        InfluxDBLogsSink {
            uri,
            token,
            protocol_version,
            namespace,
            tags,
            encoding: EncodingConfigWithDefault::default().into(),
        }
    }
}

#[cfg(feature = "influxdb-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        config::SinkContext,
        sinks::influxdb::{
            logs::InfluxDBLogsConfig,
            test_util::{onboarding_v2, BUCKET, ORG, TOKEN},
            InfluxDB2Settings,
        },
    };
    use chrono::Utc;
    use futures::stream;

    #[tokio::test]
    async fn influxdb2_logs_put_data() {
        onboarding_v2().await;

        let ns = format!("ns-{}", Utc::now().timestamp_nanos());

        let cx = SinkContext::new_test();

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
            tls: None,
        };

        let (sink, _) = config.build(cx).await.unwrap();

        let mut events = Vec::new();

        let mut event1 = Event::from("message_1");
        event1.as_mut_log().insert("host", "aws.cloud.eur");
        event1.as_mut_log().insert("source_type", "file");

        let mut event2 = Event::from("message_2");
        event2.as_mut_log().insert("host", "aws.cloud.eur");
        event2.as_mut_log().insert("source_type", "file");

        events.push(event1);
        events.push(event2);

        sink.run(stream::iter(events)).await.unwrap();

        let mut body = std::collections::HashMap::new();
        body.insert("query", format!("from(bucket:\"my-bucket\") |> range(start: 0) |> filter(fn: (r) => r._measurement == \"{}.vector\")", ns.clone()));
        body.insert("type", "flux".to_owned());

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let res = client
            .post("http://localhost:9999/api/v2/query?org=my-org")
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
