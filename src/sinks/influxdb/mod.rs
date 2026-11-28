pub mod logs;
pub mod metrics;

use bytes::Bytes;
use futures::FutureExt;
use http::{Request, StatusCode, Uri};
use serde::{Deserialize, Deserializer, de::Error as _};
use snafu::ResultExt;
use vector_http::empty_body;
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};

pub(in crate::sinks) use vector_lib::codecs::encoding::format::{
    Field, ProtocolVersion, encode_timestamp, influx_line_protocol,
};

use crate::{
    http::HttpClient,
    sinks::{HTTPRequestBuilderSnafu, util::HttpEndpoint},
};

/// Configuration settings for InfluxDB v0.x/v1.x.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct InfluxDb1Settings {
    /// The name of the database to write into.
    #[configurable(metadata(docs::examples = "vector-database"))]
    #[configurable(metadata(docs::examples = "iot-store"))]
    database: String,

    /// The consistency level to use for writes.
    #[configurable(metadata(docs::examples = "any"))]
    #[configurable(metadata(docs::examples = "one"))]
    #[configurable(metadata(docs::examples = "quorum"))]
    #[configurable(metadata(docs::examples = "all"))]
    consistency: Option<String>,

    /// The target retention policy for writes.
    #[configurable(metadata(docs::examples = "autogen"))]
    #[configurable(metadata(docs::examples = "one_day_only"))]
    retention_policy_name: Option<String>,

    /// The username to authenticate with.
    #[configurable(metadata(docs::examples = "todd"))]
    #[configurable(metadata(docs::examples = "vector-source"))]
    username: Option<String>,

    /// The password to authenticate with.
    #[configurable(metadata(docs::examples = "${INFLUXDB_PASSWORD}"))]
    #[configurable(metadata(docs::examples = "influxdb4ever"))]
    password: Option<SensitiveString>,
}

/// Configuration settings for InfluxDB v2.x.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct InfluxDb2Settings {
    /// The name of the organization to write into.
    #[configurable(metadata(docs::examples = "my-org"))]
    #[configurable(metadata(docs::examples = "33f2cff0a28e5b63"))]
    org: String,

    /// The name of the bucket to write into.
    #[configurable(metadata(docs::examples = "vector-bucket"))]
    #[configurable(metadata(docs::examples = "4d2225e4d3d49f75"))]
    bucket: String,

    /// The [token][token_docs] to authenticate with.
    ///
    /// [token_docs]: https://v2.docs.influxdata.com/v2.0/security/tokens/
    #[configurable(metadata(docs::examples = "${INFLUXDB_TOKEN}"))]
    #[configurable(metadata(docs::examples = "ef8d5de700e7989468166c40fc8a0ccd"))]
    token: SensitiveString,
}

/// InfluxDB connection settings, selected by the `version` option.
///
/// Each version only accepts its own settings.
#[configurable_component(no_deser)]
#[derive(Clone, Debug)]
#[serde(tag = "version")]
#[configurable(metadata(docs::enum_tag_description = "The InfluxDB API version to use."))]
pub enum InfluxDbSettings {
    /// InfluxDB v0.x/v1.x.
    #[serde(rename = "1")]
    V1(InfluxDb1Settings),
    /// InfluxDB v2.x.
    #[serde(rename = "2")]
    V2(InfluxDb2Settings),
}

// The derived internally tagged deserializer would match integer tags by variant index, so an
// unquoted `version: 1` would resolve to v2. Parse the tag with a plain enum instead, which only
// accepts the strings "1" and "2", and hand the remaining fields to the per-version settings.
impl<'de> Deserialize<'de> for InfluxDbSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        enum Version {
            #[serde(rename = "1")]
            V1,
            #[serde(rename = "2")]
            V2,
        }

        #[derive(Deserialize)]
        struct Tagged {
            version: Version,
            #[serde(flatten)]
            settings: serde_json::Map<String, serde_json::Value>,
        }

        let Tagged { version, settings } = Tagged::deserialize(deserializer)?;

        let settings = serde_json::Value::Object(settings);
        match version {
            Version::V1 => InfluxDb1Settings::deserialize(settings).map(Self::V1),
            Version::V2 => InfluxDb2Settings::deserialize(settings).map(Self::V2),
        }
        .map_err(D::Error::custom)
    }
}

trait InfluxDbConnection: std::fmt::Debug {
    fn write_uri(&self, endpoint: HttpEndpoint) -> crate::Result<Uri>;
    fn healthcheck_uri(&self, endpoint: HttpEndpoint) -> crate::Result<Uri>;
    fn token(&self) -> SensitiveString;
    fn protocol_version(&self) -> ProtocolVersion;
}

impl InfluxDbConnection for InfluxDb1Settings {
    fn write_uri(&self, endpoint: HttpEndpoint) -> crate::Result<Uri> {
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

    fn healthcheck_uri(&self, endpoint: HttpEndpoint) -> crate::Result<Uri> {
        encode_uri(&endpoint, "ping", &[])
    }

    fn token(&self) -> SensitiveString {
        SensitiveString::default()
    }

    fn protocol_version(&self) -> ProtocolVersion {
        ProtocolVersion::V1
    }
}

impl InfluxDbConnection for InfluxDb2Settings {
    fn write_uri(&self, endpoint: HttpEndpoint) -> crate::Result<Uri> {
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

    fn healthcheck_uri(&self, endpoint: HttpEndpoint) -> crate::Result<Uri> {
        encode_uri(&endpoint, "ping", &[])
    }

    fn token(&self) -> SensitiveString {
        self.token.clone()
    }

    fn protocol_version(&self) -> ProtocolVersion {
        ProtocolVersion::V2
    }
}

fn influxdb_settings(settings: InfluxDbSettings) -> Box<dyn InfluxDbConnection> {
    match settings {
        InfluxDbSettings::V1(settings) => Box::new(settings),
        InfluxDbSettings::V2(settings) => Box::new(settings),
    }
}

// V1: https://docs.influxdata.com/influxdb/v1.7/tools/api/#ping-http-endpoint
// V2: https://v2.docs.influxdata.com/v2.0/api/#operation/GetHealth
fn healthcheck(
    endpoint: HttpEndpoint,
    settings: InfluxDbSettings,
    client: HttpClient,
) -> crate::Result<super::Healthcheck> {
    let settings = influxdb_settings(settings);

    let uri = settings.healthcheck_uri(endpoint)?;

    let request = Request::get(uri).body(empty_body())?;

    Ok(async move {
        let response = client.send(request).await?;
        match response.status() {
            StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
            other => Err(super::HealthcheckError::unexpected_status(other).into()),
        }
    }
    .boxed())
}

/// Builds the write requests of the logs and metrics sinks.
#[derive(Clone)]
struct InfluxDbWriteRequest {
    uri: Uri,
    authorization: String,
}

impl InfluxDbWriteRequest {
    fn new(uri: Uri, token: &SensitiveString) -> Self {
        Self {
            uri,
            authorization: format!("Token {}", token.inner()),
        }
    }

    fn request(&self, body: Bytes) -> crate::Result<Request<Bytes>> {
        Request::post(self.uri.clone())
            .header("Content-Type", "text/plain")
            .header("Authorization", self.authorization.as_str())
            .body(body)
            .context(HTTPRequestBuilderSnafu)
            .map_err(Into::into)
    }
}

pub(in crate::sinks) fn encode_uri(
    endpoint: &HttpEndpoint,
    path: &str,
    pairs: &[(&str, Option<String>)],
) -> crate::Result<Uri> {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());

    for pair in pairs {
        if let Some(v) = &pair.1 {
            serializer.append_pair(pair.0, v);
        }
    }

    let query = serializer.finish();
    let path_and_query = if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    };
    Ok(endpoint.append_path(&path_and_query)?.into_uri())
}

#[cfg(test)]
#[allow(dead_code)]
pub mod test_util {
    use std::{fs::File, io::Read};

    use chrono::{DateTime, SecondsFormat, Timelike, Utc, offset::TimeZone};
    use vector_lib::{event::MetricTags, metric_tags};

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

    pub(crate) fn assert_fields(value: &str, fields: Vec<&str>) {
        let encoded_fields: Vec<&str> = value.split(',').collect();

        assert_eq!(fields.len(), encoded_fields.len());

        for field in fields {
            assert!(
                encoded_fields.contains(&field),
                "Fields: {value} has to have: {field}"
            );
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
            .get(
                reqwest::Url::parse_with_params(&format!("{endpoint}/query"), &[("q", query)])
                    .unwrap(),
            )
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
            let write_url = format!("{endpoint}/write?db={database}");
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
            status == http::StatusCode::CREATED || status == http::StatusCode::UNPROCESSABLE_ENTITY,
            "UnexpectedStatus: {status}"
        );
    }

    pub(crate) fn format_timestamp(timestamp: DateTime<Utc>, format: SecondsFormat) -> String {
        strip_timestamp(timestamp.to_rfc3339_opts(format, true))
    }

    // InfluxDB strips off trailing zeros in timestamps in metrics
    #[expect(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "Match timestamp suffixes exactly."
    )]
    fn strip_timestamp(timestamp: String) -> String {
        #[expect(
            clippy::string_slice,
            reason = "last two bytes are always ASCII ('0Z' or '.Z'), guaranteed char boundaries"
        )]
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

    #[test]
    fn test_influxdb1_settings() {
        let settings = InfluxDbSettings::V1(InfluxDb1Settings {
            database: "my-database".to_owned(),
            consistency: None,
            retention_policy_name: None,
            username: None,
            password: None,
        });
        let connection = influxdb_settings(settings);
        assert_eq!(connection.protocol_version(), ProtocolVersion::V1);
    }

    #[test]
    fn test_influxdb2_settings() {
        let settings = InfluxDbSettings::V2(InfluxDb2Settings {
            org: "my-org".to_owned(),
            bucket: "my-bucket".to_owned(),
            token: "my-token".to_owned().into(),
        });
        let connection = influxdb_settings(settings);
        assert_eq!(connection.protocol_version(), ProtocolVersion::V2);
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
            .write_uri(HttpEndpoint::parse("http://localhost:8086").unwrap())
            .unwrap();
        assert_eq!(
            "http://localhost:8086/write?consistency=quorum&db=vector_db&rp=autogen&p=secret&u=writer&precision=ns",
            uri.to_string()
        );
    }

    #[test]
    fn test_influxdb2_test_write_uri() {
        let settings = InfluxDb2Settings {
            org: "my-org".to_owned(),
            bucket: "my-bucket".to_owned(),
            token: "my-token".to_owned().into(),
        };

        let uri = settings
            .write_uri(HttpEndpoint::parse("http://localhost:9999").unwrap())
            .unwrap();
        assert_eq!(
            "http://localhost:9999/api/v2/write?org=my-org&bucket=my-bucket&precision=ns",
            uri.to_string()
        );
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
            .healthcheck_uri(HttpEndpoint::parse("http://localhost:8086").unwrap())
            .unwrap();
        assert_eq!("http://localhost:8086/ping", uri.to_string());
    }

    #[test]
    fn test_influxdb2_test_healthcheck_uri() {
        let settings = InfluxDb2Settings {
            org: "my-org".to_owned(),
            bucket: "my-bucket".to_owned(),
            token: "my-token".to_owned().into(),
        };

        let uri = settings
            .healthcheck_uri(HttpEndpoint::parse("http://localhost:9999").unwrap())
            .unwrap();
        assert_eq!("http://localhost:9999/ping", uri.to_string());
    }

    #[test]
    fn test_encode_uri_valid() {
        let uri = encode_uri(
            &HttpEndpoint::parse("http://localhost:9999").unwrap(),
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
            &HttpEndpoint::parse("http://localhost:9999/").unwrap(),
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
            &HttpEndpoint::parse("http://localhost:9999").unwrap(),
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
}

#[cfg(feature = "influxdb-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use crate::{
        config::ProxyConfig,
        http::HttpClient,
        sinks::influxdb::{
            InfluxDb1Settings, InfluxDb2Settings, InfluxDbSettings, healthcheck,
            test_util::{BUCKET, ORG, TOKEN, address_v1, address_v2, next_database, onboarding_v2},
        },
        sinks::util::HttpEndpoint,
        tls::MaybeTlsSettings,
    };

    #[tokio::test]
    async fn influxdb2_healthchecks_ok() {
        let endpoint = address_v2();
        onboarding_v2(&endpoint).await;

        let endpoint = address_v2();
        let settings = InfluxDbSettings::V2(InfluxDb2Settings {
            org: ORG.to_string(),
            bucket: BUCKET.to_string(),
            token: TOKEN.to_string().into(),
        });
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(MaybeTlsSettings::Raw(()), &proxy).unwrap();

        healthcheck(HttpEndpoint::parse(&endpoint).unwrap(), settings, client)
            .unwrap()
            .await
            .unwrap();
    }

    #[tokio::test]
    #[should_panic(expected = "called `Result::unwrap()` on an `Err` value")]
    async fn influxdb2_healthchecks_fail() {
        let endpoint = "http://127.0.0.1:9999".to_string();
        onboarding_v2(&endpoint).await;

        let settings = InfluxDbSettings::V2(InfluxDb2Settings {
            org: ORG.to_string(),
            bucket: BUCKET.to_string(),
            token: TOKEN.to_string().into(),
        });
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(MaybeTlsSettings::Raw(()), &proxy).unwrap();

        healthcheck(HttpEndpoint::parse(&endpoint).unwrap(), settings, client)
            .unwrap()
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn influxdb1_healthchecks_ok() {
        let endpoint = address_v1(false);

        let settings = InfluxDbSettings::V1(InfluxDb1Settings {
            database: next_database(),
            consistency: None,
            retention_policy_name: None,
            username: None,
            password: None,
        });
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(MaybeTlsSettings::Raw(()), &proxy).unwrap();

        healthcheck(HttpEndpoint::parse(&endpoint).unwrap(), settings, client)
            .unwrap()
            .await
            .unwrap();
    }

    #[tokio::test]
    #[should_panic(expected = "called `Result::unwrap()` on an `Err` value")]
    async fn influxdb1_healthchecks_fail() {
        let endpoint = "http://127.0.0.1:8086".to_string();
        let settings = InfluxDbSettings::V1(InfluxDb1Settings {
            database: next_database(),
            consistency: None,
            retention_policy_name: None,
            username: None,
            password: None,
        });
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(MaybeTlsSettings::Raw(()), &proxy).unwrap();

        healthcheck(HttpEndpoint::parse(&endpoint).unwrap(), settings, client)
            .unwrap()
            .await
            .unwrap();
    }
}
