pub mod logs;
pub mod metrics;

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use futures::FutureExt;
use http::{StatusCode, Uri};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tower::Service;

use crate::http::HttpClient;

pub(in crate::sinks) enum Field {
    /// string
    String(String),
    /// float
    Float(f64),
    /// unsigned integer
    UnsignedInt(u32),
    /// integer
    Int(i64),
    /// boolean
    Bool(bool),
}

#[derive(Clone, Copy, Debug)]
pub(in crate::sinks) enum ProtocolVersion {
    V1,
    V2,
}

#[derive(Debug, Snafu)]
enum ConfigError {
    #[snafu(display("InfluxDB v1 or v2 should be configured as endpoint."))]
    MissingConfiguration,
    #[snafu(display(
        "Unclear settings. Both version configured v1: {:?}, v2: {:?}.",
        v1_settings,
        v2_settings
    ))]
    BothConfiguration {
        v1_settings: InfluxDb1Settings,
        v2_settings: InfluxDb2Settings,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct InfluxDb1Settings {
    database: String,
    consistency: Option<String>,
    retention_policy_name: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct InfluxDb2Settings {
    org: String,
    bucket: String,
    token: String,
}

trait InfluxDbSettings: std::fmt::Debug {
    fn write_uri(&self, endpoint: String) -> crate::Result<Uri>;
    fn healthcheck_uri(&self, endpoint: String) -> crate::Result<Uri>;
    fn token(&self) -> String;
    fn protocol_version(&self) -> ProtocolVersion;
}

impl InfluxDbSettings for InfluxDb1Settings {
    fn write_uri(&self, endpoint: String) -> crate::Result<Uri> {
        encode_uri(
            &endpoint,
            "write",
            &[
                ("consistency", self.consistency.clone()),
                ("db", Some(self.database.clone())),
                ("rp", self.retention_policy_name.clone()),
                ("p", self.password.clone()),
                ("u", self.username.clone()),
                ("precision", Some("ns".to_owned())),
            ],
        )
    }

    fn healthcheck_uri(&self, endpoint: String) -> crate::Result<Uri> {
        encode_uri(&endpoint, "ping", &[])
    }

    fn token(&self) -> String {
        "".to_string()
    }

    fn protocol_version(&self) -> ProtocolVersion {
        ProtocolVersion::V1
    }
}

impl InfluxDbSettings for InfluxDb2Settings {
    fn write_uri(&self, endpoint: String) -> crate::Result<Uri> {
        encode_uri(
            &endpoint,
            "api/v2/write",
            &[
                ("org", Some(self.org.clone())),
                ("bucket", Some(self.bucket.clone())),
                ("precision", Some("ns".to_owned())),
            ],
        )
    }

    fn healthcheck_uri(&self, endpoint: String) -> crate::Result<Uri> {
        encode_uri(&endpoint, "ping", &[])
    }

    fn token(&self) -> String {
        self.token.clone()
    }

    fn protocol_version(&self) -> ProtocolVersion {
        ProtocolVersion::V2
    }
}

fn influxdb_settings(
    influxdb1_settings: Option<InfluxDb1Settings>,
    influxdb2_settings: Option<InfluxDb2Settings>,
) -> Result<Box<dyn InfluxDbSettings>, crate::Error> {
    match (influxdb1_settings, influxdb2_settings) {
        (Some(v1_settings), Some(v2_settings)) => Err(ConfigError::BothConfiguration {
            v1_settings,
            v2_settings,
        }
        .into()),
        (None, None) => Err(ConfigError::MissingConfiguration.into()),
        (Some(settings), _) => Ok(Box::new(settings)),
        (_, Some(settings)) => Ok(Box::new(settings)),
    }
}

// V1: https://docs.influxdata.com/influxdb/v1.7/tools/api/#ping-http-endpoint
// V2: https://v2.docs.influxdata.com/v2.0/api/#operation/GetHealth
fn healthcheck(
    endpoint: String,
    influxdb1_settings: Option<InfluxDb1Settings>,
    influxdb2_settings: Option<InfluxDb2Settings>,
    mut client: HttpClient,
) -> crate::Result<super::Healthcheck> {
    let settings = influxdb_settings(influxdb1_settings, influxdb2_settings)?;

    let uri = settings.healthcheck_uri(endpoint)?;

    let request = hyper::Request::get(uri).body(hyper::Body::empty()).unwrap();

    Ok(async move {
        client
            .call(request)
            .await
            .map_err(|error| error.into())
            .and_then(|response| match response.status() {
                StatusCode::OK => Ok(()),
                StatusCode::NO_CONTENT => Ok(()),
                other => Err(super::HealthcheckError::UnexpectedStatus { status: other }.into()),
            })
    }
    .boxed())
}

// https://v2.docs.influxdata.com/v2.0/reference/syntax/line-protocol/
pub(in crate::sinks) fn influx_line_protocol(
    protocol_version: ProtocolVersion,
    measurement: &str,
    tags: Option<BTreeMap<String, String>>,
    fields: Option<HashMap<String, Field>>,
    timestamp: i64,
    line_protocol: &mut String,
) -> Result<(), &'static str> {
    // Fields
    let unwrapped_fields = fields.unwrap_or_else(HashMap::new);
    // LineProtocol should have a field
    if unwrapped_fields.is_empty() {
        return Err("fields must not be empty");
    }

    encode_string(measurement, line_protocol);
    line_protocol.push(',');

    // Tags
    let unwrapped_tags = tags.unwrap_or_default();
    encode_tags(unwrapped_tags, line_protocol);
    line_protocol.push(' ');

    // Fields
    encode_fields(protocol_version, unwrapped_fields, line_protocol);
    line_protocol.push(' ');

    // Timestamp
    line_protocol.push_str(&timestamp.to_string());
    line_protocol.push('\n');
    Ok(())
}

fn encode_tags(tags: BTreeMap<String, String>, output: &mut String) {
    // `tags` is already sorted
    for (key, value) in tags {
        if key.is_empty() || value.is_empty() {
            continue;
        }
        encode_string(&key, output);
        output.push('=');
        encode_string(&value, output);
        output.push(',');
    }

    // remove last ','
    output.pop();
}

fn encode_fields(
    protocol_version: ProtocolVersion,
    fields: HashMap<String, Field>,
    output: &mut String,
) {
    for (key, value) in fields.into_iter() {
        encode_string(&key, output);
        output.push('=');
        match value {
            Field::String(s) => {
                output.push('"');
                for c in s.chars() {
                    if "\\\"".contains(c) {
                        output.push('\\');
                    }
                    output.push(c);
                }
                output.push('"');
            }
            Field::Float(f) => output.push_str(&f.to_string()),
            Field::UnsignedInt(i) => {
                output.push_str(&i.to_string());
                let c = match protocol_version {
                    ProtocolVersion::V1 => 'i',
                    ProtocolVersion::V2 => 'u',
                };
                output.push(c);
            }
            Field::Int(i) => {
                output.push_str(&i.to_string());
                output.push('i');
            }
            Field::Bool(b) => {
                output.push_str(&b.to_string());
            }
        };
        output.push(',');
    }

    // remove last ','
    output.pop();
}

fn encode_string(key: &str, output: &mut String) {
    for c in key.chars() {
        if "\\, =".contains(c) {
            output.push('\\');
        }
        output.push(c);
    }
}

pub(in crate::sinks) fn encode_timestamp(timestamp: Option<DateTime<Utc>>) -> i64 {
    if let Some(ts) = timestamp {
        ts.timestamp_nanos()
    } else {
        encode_timestamp(Some(Utc::now()))
    }
}

pub(in crate::sinks) fn encode_uri(
    endpoint: &str,
    path: &str,
    pairs: &[(&str, Option<String>)],
) -> crate::Result<Uri> {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());

    for pair in pairs {
        if let Some(v) = &pair.1 {
            serializer.append_pair(pair.0, v);
        }
    }

    let mut url = if endpoint.ends_with('/') {
        format!("{}{}?{}", endpoint, path, serializer.finish())
    } else {
        format!("{}/{}?{}", endpoint, path, serializer.finish())
    };

    if url.ends_with('?') {
        url.pop();
    }

    Ok(url.parse::<Uri>().context(super::UriParseError)?)
}

#[cfg(test)]
#[allow(dead_code)]
pub mod test_util {
    use std::{fs::File, io::Read};

    use chrono::{offset::TimeZone, DateTime, SecondsFormat, Utc};

    use super::*;
    use crate::tls;

    pub(crate) const ORG: &str = "my-org";
    pub(crate) const BUCKET: &str = "my-bucket";
    pub(crate) const TOKEN: &str = "my-token";

    pub(crate) fn next_database() -> String {
        format!("testdb{}", Utc::now().timestamp_nanos())
    }

    pub(crate) fn ts() -> DateTime<Utc> {
        Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)
    }

    pub(crate) fn tags() -> BTreeMap<String, String> {
        vec![
            ("normal_tag".to_owned(), "value".to_owned()),
            ("true_tag".to_owned(), "true".to_owned()),
            ("empty_tag".to_owned(), "".to_owned()),
        ]
        .into_iter()
        .collect()
    }

    pub(crate) fn assert_fields(value: String, fields: Vec<&str>) {
        let encoded_fields: Vec<&str> = value.split(',').collect();

        assert_eq!(fields.len(), encoded_fields.len());

        for field in fields.into_iter() {
            assert!(
                encoded_fields.contains(&field),
                "Fields: {} has to have: {}",
                value,
                field
            )
        }
    }

    // ns.requests,metric_type=distribution,normal_tag=value,true_tag=true avg=1.875,count=8,max=3,median=2,min=1,quantile_0.95=3,sum=15 1542182950000000011
    //
    // =>
    //
    // ns.requests
    // metric_type=distribution,normal_tag=value,true_tag=true
    // avg=1.875,count=8,max=3,median=2,min=1,quantile_0.95=3,sum=15
    // 1542182950000000011
    //
    pub(crate) fn split_line_protocol(line_protocol: &str) -> (&str, &str, String, &str) {
        let mut split = line_protocol.splitn(2, ',').collect::<Vec<&str>>();
        let measurement = split[0];

        split = split[1].splitn(3, ' ').collect::<Vec<&str>>();

        (measurement, split[0], split[1].to_string(), split[2])
    }

    fn client() -> reqwest::Client {
        let mut test_ca = Vec::<u8>::new();
        File::open(tls::TEST_PEM_CA_PATH)
            .unwrap()
            .read_to_end(&mut test_ca)
            .unwrap();
        let test_ca = reqwest::Certificate::from_pem(&test_ca).unwrap();

        reqwest::Client::builder()
            .add_root_certificate(test_ca)
            .build()
            .unwrap()
    }

    pub(crate) async fn query_v1(endpoint: &str, query: &str) -> reqwest::Response {
        client()
            .get(&format!("{}/query", endpoint))
            .query(&[("q", query)])
            .send()
            .await
            .unwrap()
    }

    pub(crate) async fn onboarding_v1(endpoint: &str) -> String {
        let database = next_database();
        let status = query_v1(endpoint, &format!("create database {}", database))
            .await
            .status();
        assert_eq!(status, http::StatusCode::OK, "UnexpectedStatus: {}", status);
        // Some times InfluxDB will return OK before it can actually
        // accept writes to the database, leading to test failures. Test
        // this with empty writes and loop if it reports the database
        // does not exist yet.
        crate::test_util::wait_for(|| {
            let write_url = format!("{}/write?db={}", endpoint, &database);
            async move {
                match client()
                    .post(&write_url)
                    .header("Content-Type", "text/plain")
                    .header("Authorization", &format!("Token {}", TOKEN))
                    .body("")
                    .send()
                    .await
                    .unwrap()
                    .status()
                {
                    http::StatusCode::NO_CONTENT => true,
                    http::StatusCode::NOT_FOUND => false,
                    status => panic!("Unexpected status: {}", status),
                }
            }
        })
        .await;
        database
    }

    pub(crate) async fn cleanup_v1(endpoint: &str, database: &str) {
        let status = query_v1(endpoint, &format!("drop database {}", database))
            .await
            .status();
        assert_eq!(status, http::StatusCode::OK, "UnexpectedStatus: {}", status);
    }

    pub(crate) async fn onboarding_v2() {
        let mut body = std::collections::HashMap::new();
        body.insert("username", "my-user");
        body.insert("password", "my-password");
        body.insert("org", ORG);
        body.insert("bucket", BUCKET);
        body.insert("token", TOKEN);

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let res = client
            .post("http://localhost:9999/api/v2/setup")
            .json(&body)
            .header("accept", "application/json")
            .send()
            .await
            .unwrap();

        let status = res.status();

        assert!(
            status == StatusCode::CREATED || status == StatusCode::UNPROCESSABLE_ENTITY,
            "UnexpectedStatus: {}",
            status
        );
    }

    pub(crate) fn format_timestamp(timestamp: DateTime<Utc>, format: SecondsFormat) -> String {
        strip_timestamp(timestamp.to_rfc3339_opts(format, true))
    }

    // InfluxDB strips off trailing zeros in timestamps in metrics
    fn strip_timestamp(timestamp: String) -> String {
        let strip_one = || format!("{}Z", &timestamp[..timestamp.len() - 2]);
        match timestamp {
            _ if timestamp.ends_with("0Z") => strip_timestamp(strip_one()),
            _ if timestamp.ends_with(".Z") => strip_one(),
            _ => timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::influxdb::test_util::{assert_fields, tags, ts};

    #[derive(Deserialize, Serialize, Debug, Clone, Default)]
    #[serde(deny_unknown_fields)]
    pub struct InfluxDbTestConfig {
        #[serde(flatten)]
        pub influxdb1_settings: Option<InfluxDb1Settings>,
        #[serde(flatten)]
        pub influxdb2_settings: Option<InfluxDb2Settings>,
    }

    #[test]
    fn test_influxdb_settings_both() {
        let config = r#"
        bucket = "my-bucket"
        org = "my-org"
        token = "my-token"
        database = "my-database"
    "#;
        let config: InfluxDbTestConfig = toml::from_str(config).unwrap();
        let settings = influxdb_settings(config.influxdb1_settings, config.influxdb2_settings);
        assert_eq!(
            format!("{}", settings.expect_err("expected error")),
            "Unclear settings. Both version configured v1: InfluxDb1Settings { database: \"my-database\", consistency: None, retention_policy_name: None, username: None, password: None }, v2: InfluxDb2Settings { org: \"my-org\", bucket: \"my-bucket\", token: \"my-token\" }.".to_owned()
        );
    }

    #[test]
    fn test_influxdb_settings_missing() {
        let config = r#"
    "#;
        let config: InfluxDbTestConfig = toml::from_str(config).unwrap();
        let settings = influxdb_settings(config.influxdb1_settings, config.influxdb2_settings);
        assert_eq!(
            format!("{}", settings.expect_err("expected error")),
            "InfluxDB v1 or v2 should be configured as endpoint.".to_owned()
        );
    }

    #[test]
    fn test_influxdb1_settings() {
        let config = r#"
        database = "my-database"
    "#;
        let config: InfluxDbTestConfig = toml::from_str(config).unwrap();
        let _ = influxdb_settings(config.influxdb1_settings, config.influxdb2_settings).unwrap();
    }

    #[test]
    fn test_influxdb2_settings() {
        let config = r#"
        bucket = "my-bucket"
        org = "my-org"
        token = "my-token"
    "#;
        let config: InfluxDbTestConfig = toml::from_str(config).unwrap();
        let _ = influxdb_settings(config.influxdb1_settings, config.influxdb2_settings).unwrap();
    }

    #[test]
    fn test_influxdb1_test_write_uri() {
        let settings = InfluxDb1Settings {
            consistency: Some("quorum".to_owned()),
            database: "vector_db".to_owned(),
            retention_policy_name: Some("autogen".to_owned()),
            username: Some("writer".to_owned()),
            password: Some("secret".to_owned()),
        };

        let uri = settings
            .write_uri("http://localhost:8086".to_owned())
            .unwrap();
        assert_eq!("http://localhost:8086/write?consistency=quorum&db=vector_db&rp=autogen&p=secret&u=writer&precision=ns", uri.to_string())
    }

    #[test]
    fn test_influxdb2_test_write_uri() {
        let settings = InfluxDb2Settings {
            org: "my-org".to_owned(),
            bucket: "my-bucket".to_owned(),
            token: "my-token".to_owned(),
        };

        let uri = settings
            .write_uri("http://localhost:9999".to_owned())
            .unwrap();
        assert_eq!(
            "http://localhost:9999/api/v2/write?org=my-org&bucket=my-bucket&precision=ns",
            uri.to_string()
        )
    }

    #[test]
    fn test_influxdb1_test_healthcheck_uri() {
        let settings = InfluxDb1Settings {
            consistency: Some("quorum".to_owned()),
            database: "vector_db".to_owned(),
            retention_policy_name: Some("autogen".to_owned()),
            username: Some("writer".to_owned()),
            password: Some("secret".to_owned()),
        };

        let uri = settings
            .healthcheck_uri("http://localhost:8086".to_owned())
            .unwrap();
        assert_eq!("http://localhost:8086/ping", uri.to_string())
    }

    #[test]
    fn test_influxdb2_test_healthcheck_uri() {
        let settings = InfluxDb2Settings {
            org: "my-org".to_owned(),
            bucket: "my-bucket".to_owned(),
            token: "my-token".to_owned(),
        };

        let uri = settings
            .healthcheck_uri("http://localhost:9999".to_owned())
            .unwrap();
        assert_eq!("http://localhost:9999/ping", uri.to_string())
    }

    #[test]
    fn test_encode_tags() {
        let mut value = String::new();
        encode_tags(tags(), &mut value);

        assert_eq!(value, "normal_tag=value,true_tag=true");

        let tags_to_escape = vec![
            ("tag".to_owned(), "val=ue".to_owned()),
            ("name escape".to_owned(), "true".to_owned()),
            ("value_escape".to_owned(), "value escape".to_owned()),
            ("a_first_place".to_owned(), "10".to_owned()),
        ]
        .into_iter()
        .collect();

        let mut value = String::new();
        encode_tags(tags_to_escape, &mut value);
        assert_eq!(
            value,
            "a_first_place=10,name\\ escape=true,tag=val\\=ue,value_escape=value\\ escape"
        );
    }

    #[test]
    fn tags_order() {
        let mut value = String::new();
        encode_tags(
            vec![
                ("a", "value"),
                ("b", "value"),
                ("c", "value"),
                ("d", "value"),
                ("e", "value"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect(),
            &mut value,
        );
        assert_eq!(value, "a=value,b=value,c=value,d=value,e=value");
    }

    #[test]
    fn test_encode_fields_v1() {
        let fields = vec![
            (
                "field_string".to_owned(),
                Field::String("string value".to_owned()),
            ),
            (
                "field_string_escape".to_owned(),
                Field::String("string\\val\"ue".to_owned()),
            ),
            ("field_float".to_owned(), Field::Float(123.45)),
            ("field_unsigned_int".to_owned(), Field::UnsignedInt(657)),
            ("field_int".to_owned(), Field::Int(657646)),
            ("field_bool_true".to_owned(), Field::Bool(true)),
            ("field_bool_false".to_owned(), Field::Bool(false)),
            ("escape key".to_owned(), Field::Float(10.0)),
        ]
        .into_iter()
        .collect();

        let mut value = String::new();
        encode_fields(ProtocolVersion::V1, fields, &mut value);
        assert_fields(
            value,
            [
                "escape\\ key=10",
                "field_float=123.45",
                "field_string=\"string value\"",
                "field_string_escape=\"string\\\\val\\\"ue\"",
                "field_unsigned_int=657i",
                "field_int=657646i",
                "field_bool_true=true",
                "field_bool_false=false",
            ]
            .to_vec(),
        )
    }

    #[test]
    fn test_encode_fields() {
        let fields = vec![
            (
                "field_string".to_owned(),
                Field::String("string value".to_owned()),
            ),
            (
                "field_string_escape".to_owned(),
                Field::String("string\\val\"ue".to_owned()),
            ),
            ("field_float".to_owned(), Field::Float(123.45)),
            ("field_unsigned_int".to_owned(), Field::UnsignedInt(657)),
            ("field_int".to_owned(), Field::Int(657646)),
            ("field_bool_true".to_owned(), Field::Bool(true)),
            ("field_bool_false".to_owned(), Field::Bool(false)),
            ("escape key".to_owned(), Field::Float(10.0)),
        ]
        .into_iter()
        .collect();

        let mut value = String::new();
        encode_fields(ProtocolVersion::V2, fields, &mut value);
        assert_fields(
            value,
            [
                "escape\\ key=10",
                "field_float=123.45",
                "field_string=\"string value\"",
                "field_string_escape=\"string\\\\val\\\"ue\"",
                "field_unsigned_int=657u",
                "field_int=657646i",
                "field_bool_true=true",
                "field_bool_false=false",
            ]
            .to_vec(),
        )
    }

    #[test]
    fn test_encode_string() {
        let mut value = String::new();
        encode_string("measurement_name", &mut value);
        assert_eq!(value, "measurement_name");

        let mut value = String::new();
        encode_string("measurement name", &mut value);
        assert_eq!(value, "measurement\\ name");

        let mut value = String::new();
        encode_string("measurement=name", &mut value);
        assert_eq!(value, "measurement\\=name");

        let mut value = String::new();
        encode_string("measurement,name", &mut value);
        assert_eq!(value, "measurement\\,name");
    }

    #[test]
    fn test_encode_timestamp() {
        let start = Utc::now().timestamp_nanos();
        assert_eq!(encode_timestamp(Some(ts())), 1542182950000000011);
        assert!(encode_timestamp(None) >= start)
    }

    #[test]
    fn test_encode_uri_valid() {
        let uri = encode_uri(
            "http://localhost:9999",
            "api/v2/write",
            &[
                ("org", Some("my-org".to_owned())),
                ("bucket", Some("my-bucket".to_owned())),
                ("precision", Some("ns".to_owned())),
            ],
        )
        .unwrap();
        assert_eq!(
            uri,
            "http://localhost:9999/api/v2/write?org=my-org&bucket=my-bucket&precision=ns"
        );

        let uri = encode_uri(
            "http://localhost:9999/",
            "api/v2/write",
            &[
                ("org", Some("my-org".to_owned())),
                ("bucket", Some("my-bucket".to_owned())),
            ],
        )
        .unwrap();
        assert_eq!(
            uri,
            "http://localhost:9999/api/v2/write?org=my-org&bucket=my-bucket"
        );

        let uri = encode_uri(
            "http://localhost:9999",
            "api/v2/write",
            &[
                ("org", Some("Orgazniation name".to_owned())),
                ("bucket", Some("Bucket=name".to_owned())),
                ("none", None),
            ],
        )
        .unwrap();
        assert_eq!(
            uri,
            "http://localhost:9999/api/v2/write?org=Orgazniation+name&bucket=Bucket%3Dname"
        );
    }

    #[test]
    fn test_encode_uri_invalid() {
        encode_uri(
            "localhost:9999",
            "api/v2/write",
            &[
                ("org", Some("my-org".to_owned())),
                ("bucket", Some("my-bucket".to_owned())),
            ],
        )
        .unwrap_err();
    }
}

#[cfg(feature = "influxdb-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use crate::{
        config::ProxyConfig,
        http::HttpClient,
        sinks::influxdb::{
            healthcheck,
            test_util::{next_database, onboarding_v2, BUCKET, ORG, TOKEN},
            InfluxDb1Settings, InfluxDb2Settings,
        },
    };

    #[tokio::test]
    async fn influxdb2_healthchecks_ok() {
        onboarding_v2().await;

        let endpoint = "http://localhost:9999".to_string();
        let influxdb1_settings = None;
        let influxdb2_settings = Some(InfluxDb2Settings {
            org: ORG.to_string(),
            bucket: BUCKET.to_string(),
            token: TOKEN.to_string(),
        });
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(None, &proxy).unwrap();

        healthcheck(endpoint, influxdb1_settings, influxdb2_settings, client)
            .unwrap()
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn influxdb2_healthchecks_fail() {
        onboarding_v2().await;

        let endpoint = "http://not_exist:9999".to_string();
        let influxdb1_settings = None;
        let influxdb2_settings = Some(InfluxDb2Settings {
            org: ORG.to_string(),
            bucket: BUCKET.to_string(),
            token: TOKEN.to_string(),
        });
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(None, &proxy).unwrap();

        healthcheck(endpoint, influxdb1_settings, influxdb2_settings, client)
            .unwrap()
            .await
            .unwrap_err();
    }

    #[tokio::test]
    async fn influxdb1_healthchecks_ok() {
        let endpoint = "http://localhost:8086".to_string();
        let influxdb1_settings = Some(InfluxDb1Settings {
            database: next_database(),
            consistency: None,
            retention_policy_name: None,
            username: None,
            password: None,
        });
        let influxdb2_settings = None;
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(None, &proxy).unwrap();

        healthcheck(endpoint, influxdb1_settings, influxdb2_settings, client)
            .unwrap()
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn influxdb1_healthchecks_fail() {
        let endpoint = "http://not_exist:8086".to_string();
        let influxdb1_settings = Some(InfluxDb1Settings {
            database: next_database(),
            consistency: None,
            retention_policy_name: None,
            username: None,
            password: None,
        });
        let influxdb2_settings = None;
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(None, &proxy).unwrap();

        healthcheck(endpoint, influxdb1_settings, influxdb2_settings, client)
            .unwrap()
            .await
            .unwrap_err();
    }
}
