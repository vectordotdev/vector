pub mod logs;
pub mod metrics;

use futures::FutureExt;
use http::{StatusCode, Uri};
use snafu::{ResultExt, Snafu};
use tower::Service;
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};

pub use vector_lib::codecs::encoding::format::{
    Field, ProtocolVersion, encode_timestamp, influx_line_protocol,
};

use crate::http::HttpClient;

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

/// Configuration settings for InfluxDB v0.x/v1.x.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct InfluxDb1Settings {
    /// The name of the database to write into.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "vector-database"))]
    #[configurable(metadata(docs::examples = "iot-store"))]
    database: String,

    /// The consistency level to use for writes.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "any"))]
    #[configurable(metadata(docs::examples = "one"))]
    #[configurable(metadata(docs::examples = "quorum"))]
    #[configurable(metadata(docs::examples = "all"))]
    consistency: Option<String>,

    /// The target retention policy for writes.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "autogen"))]
    #[configurable(metadata(docs::examples = "one_day_only"))]
    retention_policy_name: Option<String>,

    /// The username to authenticate with.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "todd"))]
    #[configurable(metadata(docs::examples = "vector-source"))]
    username: Option<String>,

    /// The password to authenticate with.
    ///
    /// Only relevant when using InfluxDB v0.x/v1.x.
    #[configurable(metadata(docs::examples = "${INFLUXDB_PASSWORD}"))]
    #[configurable(metadata(docs::examples = "influxdb4ever"))]
    password: Option<SensitiveString>,
}

/// Configuration settings for InfluxDB v2.x.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct InfluxDb2Settings {
    /// The name of the organization to write into.
    ///
    /// Only relevant when using InfluxDB v2.x and above.
    #[configurable(metadata(docs::examples = "my-org"))]
    #[configurable(metadata(docs::examples = "33f2cff0a28e5b63"))]
    org: String,

    /// The name of the bucket to write into.
    ///
    /// Only relevant when using InfluxDB v2.x and above.
    #[configurable(metadata(docs::examples = "vector-bucket"))]
    #[configurable(metadata(docs::examples = "4d2225e4d3d49f75"))]
    bucket: String,

    /// The [token][token_docs] to authenticate with.
    ///
    /// Only relevant when using InfluxDB v2.x and above.
    ///
    /// [token_docs]: https://v2.docs.influxdata.com/v2.0/security/tokens/
    #[configurable(metadata(docs::examples = "${INFLUXDB_TOKEN}"))]
    #[configurable(metadata(docs::examples = "ef8d5de700e7989468166c40fc8a0ccd"))]
    token: SensitiveString,
}

trait InfluxDbSettings: std::fmt::Debug {
    fn write_uri(&self, endpoint: String) -> crate::Result<Uri>;
    fn healthcheck_uri(&self, endpoint: String) -> crate::Result<Uri>;
    fn token(&self) -> SensitiveString;
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
                ("p", self.password.as_ref().map(|v| v.inner().to_owned())),
                ("u", self.username.clone()),
                ("precision", Some("ns".to_owned())),
            ],
        )
    }

    fn healthcheck_uri(&self, endpoint: String) -> crate::Result<Uri> {
        encode_uri(&endpoint, "ping", &[])
    }

    fn token(&self) -> SensitiveString {
        SensitiveString::default()
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

    fn token(&self) -> SensitiveString {
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

    Ok(url.parse::<Uri>().context(super::UriParseSnafu)?)
}

#[cfg(test)]
#[allow(dead_code)]
pub mod test_util {
    use std::{fs::File, io::Read};

    use chrono::{DateTime, SecondsFormat, Timelike, Utc, offset::TimeZone};
    use vector_lib::event::MetricTags;
    use vector_lib::metric_tags;

    use super::*;
    use crate::tls;

    pub(crate) const ORG: &str = "my-org";
    pub(crate) const BUCKET: &str = "my-bucket";
    pub(crate) const TOKEN: &str = "my-token";

    pub(crate) fn next_database() -> String {
        format!("testdb{}", Utc::now().timestamp_nanos_opt().unwrap())
    }

    pub(crate) fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
            .single()
            .and_then(|t| t.with_nanosecond(11))
            .expect("invalid timestamp")
    }

    pub(crate) fn tags() -> MetricTags {
        metric_tags!(
            "normal_tag" => "value",
            "true_tag" => "true",
            "empty_tag" => "",
        )
    }

    pub(crate) fn assert_fields(value: String, fields: Vec<&str>) {
        let encoded_fields: Vec<&str> = value.split(',').collect();

        assert_eq!(fields.len(), encoded_fields.len());

        for field in fields.into_iter() {
            assert!(
                encoded_fields.contains(&field),
                "Fields: {value} has to have: {field}"
            )
        }
    }

    pub(crate) fn address_v1(secure: bool) -> String {
        if secure {
            std::env::var("INFLUXDB_V1_HTTPS_ADDRESS")
                .unwrap_or_else(|_| "http://localhost:8087".into())
        } else {
            std::env::var("INFLUXDB_V1_HTTP_ADDRESS")
                .unwrap_or_else(|_| "http://localhost:8086".into())
        }
    }

    pub(crate) fn address_v2() -> String {
        std::env::var("INFLUXDB_V2_ADDRESS").unwrap_or_else(|_| "http://localhost:9999".into())
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
        let (name, fields) = line_protocol.split_once(' ').unwrap_or_default();
        // tags and timestamp may not be present
        let (measurement, tags) = name.split_once(',').unwrap_or((name, ""));
        let (fields, ts) = fields.split_once(' ').unwrap_or((fields, ""));

        (measurement, tags, fields.to_string(), ts)
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
            .get(format!("{endpoint}/query"))
            .query(&[("q", query)])
            .send()
            .await
            .unwrap()
    }

    pub(crate) async fn onboarding_v1(endpoint: &str) -> String {
        let database = next_database();
        let status = query_v1(endpoint, &format!("create database {database}"))
            .await
            .status();
        assert_eq!(status, http::StatusCode::OK, "UnexpectedStatus: {status}");
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
                    .header("Authorization", &format!("Token {TOKEN}"))
                    .body("")
                    .send()
                    .await
                    .unwrap()
                    .status()
                {
                    http::StatusCode::NO_CONTENT => true,
                    http::StatusCode::NOT_FOUND => false,
                    status => panic!("Unexpected status: {status}"),
                }
            }
        })
        .await;
        database
    }

    pub(crate) async fn cleanup_v1(endpoint: &str, database: &str) {
        let status = query_v1(endpoint, &format!("drop database {database}"))
            .await
            .status();
        assert_eq!(status, http::StatusCode::OK, "UnexpectedStatus: {status}");
    }

    pub(crate) async fn onboarding_v2(endpoint: &str) {
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
            .post(format!("{endpoint}/api/v2/setup"))
            .json(&body)
            .header("accept", "application/json")
            .send()
            .await
            .unwrap();

        let status = res.status();

        assert!(
            status == StatusCode::CREATED || status == StatusCode::UNPROCESSABLE_ENTITY,
            "UnexpectedStatus: {status}"
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
    use serde::{Deserialize, Serialize};

    use super::*;
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
            settings.expect_err("expected error").to_string(),
            "Unclear settings. Both version configured v1: InfluxDb1Settings { database: \"my-database\", consistency: None, retention_policy_name: None, username: None, password: None }, v2: InfluxDb2Settings { org: \"my-org\", bucket: \"my-bucket\", token: \"**REDACTED**\" }.".to_owned()
        );
    }

    #[test]
    fn test_influxdb_settings_missing() {
        let config = r"
    ";
        let config: InfluxDbTestConfig = toml::from_str(config).unwrap();
        let settings = influxdb_settings(config.influxdb1_settings, config.influxdb2_settings);
        assert_eq!(
            settings.expect_err("expected error").to_string(),
            "InfluxDB v1 or v2 should be configured as endpoint.".to_owned()
        );
    }

    #[test]
    fn test_influxdb1_settings() {
        let config = r#"
        database = "my-database"
    "#;
        let config: InfluxDbTestConfig = toml::from_str(config).unwrap();
        _ = influxdb_settings(config.influxdb1_settings, config.influxdb2_settings).unwrap();
    }

    #[test]
    fn test_influxdb2_settings() {
        let config = r#"
        bucket = "my-bucket"
        org = "my-org"
        token = "my-token"
    "#;
        let config: InfluxDbTestConfig = toml::from_str(config).unwrap();
        _ = influxdb_settings(config.influxdb1_settings, config.influxdb2_settings).unwrap();
    }

    #[test]
    fn test_influxdb1_test_write_uri() {
        let settings = InfluxDb1Settings {
            consistency: Some("quorum".to_owned()),
            database: "vector_db".to_owned(),
            retention_policy_name: Some("autogen".to_owned()),
            username: Some("writer".to_owned()),
            password: Some("secret".to_owned().into()),
        };

        let uri = settings
            .write_uri("http://localhost:8086".to_owned())
            .unwrap();
        assert_eq!(
            "http://localhost:8086/write?consistency=quorum&db=vector_db&rp=autogen&p=secret&u=writer&precision=ns",
            uri.to_string()
        )
    }

    #[test]
    fn test_influxdb2_test_write_uri() {
        let settings = InfluxDb2Settings {
            org: "my-org".to_owned(),
            bucket: "my-bucket".to_owned(),
            token: "my-token".to_owned().into(),
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
            password: Some("secret".to_owned().into()),
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
            token: "my-token".to_owned().into(),
        };

        let uri = settings
            .healthcheck_uri("http://localhost:9999".to_owned())
            .unwrap();
        assert_eq!("http://localhost:9999/ping", uri.to_string())
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
                ("org", Some("Organization name".to_owned())),
                ("bucket", Some("Bucket=name".to_owned())),
                ("none", None),
            ],
        )
        .unwrap();
        assert_eq!(
            uri,
            "http://localhost:9999/api/v2/write?org=Organization+name&bucket=Bucket%3Dname"
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
            InfluxDb1Settings, InfluxDb2Settings, healthcheck,
            test_util::{BUCKET, ORG, TOKEN, address_v1, address_v2, next_database, onboarding_v2},
        },
    };

    #[tokio::test]
    async fn influxdb2_healthchecks_ok() {
        let endpoint = address_v2();
        onboarding_v2(&endpoint).await;

        let endpoint = address_v2();
        let influxdb1_settings = None;
        let influxdb2_settings = Some(InfluxDb2Settings {
            org: ORG.to_string(),
            bucket: BUCKET.to_string(),
            token: TOKEN.to_string().into(),
        });
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(None, &proxy).unwrap();

        healthcheck(endpoint, influxdb1_settings, influxdb2_settings, client)
            .unwrap()
            .await
            .unwrap()
    }

    #[tokio::test]
    #[should_panic]
    async fn influxdb2_healthchecks_fail() {
        let endpoint = "http://127.0.0.1:9999".to_string();
        onboarding_v2(&endpoint).await;

        let influxdb1_settings = None;
        let influxdb2_settings = Some(InfluxDb2Settings {
            org: ORG.to_string(),
            bucket: BUCKET.to_string(),
            token: TOKEN.to_string().into(),
        });
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(None, &proxy).unwrap();

        healthcheck(endpoint, influxdb1_settings, influxdb2_settings, client)
            .unwrap()
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn influxdb1_healthchecks_ok() {
        let endpoint = address_v1(false);

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
    #[should_panic]
    async fn influxdb1_healthchecks_fail() {
        let endpoint = "http://127.0.0.1:8086".to_string();
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
}
