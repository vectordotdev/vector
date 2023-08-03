pub mod logs;
pub mod metrics;

use base64::Engine;
use chrono::Utc;
use futures::FutureExt;
use http::{StatusCode, Uri};
use snafu::ResultExt;
use std::collections::HashMap;
use tower::Service;
use vector_config::configurable_component;
use vrl::value::Value;

use crate::http::HttpClient;

pub const DEFAULT_DATABASE: &str = "public";
pub const DEFAULT_CATALOG: &str = "cnosdb";
pub const DEFAULT_USER: &str = "root";
pub const DEFAULT_PASSWORD: &str = "";

pub const TYPE_TAG_KEY: &str = "metric_type";

fn default_dbname() -> String {
    DEFAULT_DATABASE.to_string()
}

fn default_tenant() -> String {
    DEFAULT_CATALOG.to_string()
}

fn default_user() -> String {
    DEFAULT_USER.to_string()
}

fn default_pwd() -> String {
    DEFAULT_PASSWORD.to_string()
}

/// Configuration for the `cnosdb` sink.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
pub struct CnosDBSettings {
    /// The name of the database to write into.
    /// Default value is `public`.
    #[configurable(metadata(docs::examples = "public"))]
    #[derivative(Default(value = "default_dbname()"))]
    #[serde(default = "default_dbname")]
    database: String,

    /// The name of the tenant to write into. Default value is `cnosdb`.
    #[configurable(metadata(docs::examples = "cnosdb"))]
    #[derivative(Default(value = "default_tenant()"))]
    #[serde(default = "default_tenant")]
    tenant: String,

    /// The user to use for authentication. Default value is `root`.
    #[configurable(metadata(docs::examples = "root"))]
    #[derivative(Default(value = "default_user()"))]
    #[serde(default = "default_user")]
    user: String,

    /// The password to use for authentication. Default value is empty.
    #[configurable(metadata(docs::examples = ""))]
    #[derivative(Default(value = "default_pwd()"))]
    #[serde(default = "default_pwd")]
    password: String,
}

impl CnosDBSettings {
    pub fn write_uri(&self, mut endpoint: String) -> crate::Result<Uri> {
        if !endpoint.ends_with('/') {
            endpoint.push('/');
        }
        let mut url = format!("{}api/v1/write?", endpoint);
        url.push_str("db=");
        url.push_str(&self.database);
        url.push_str("&tenant=");
        url.push_str(&self.tenant);

        Ok(url.parse::<Uri>().context(super::UriParseSnafu)?)
    }

    pub fn healthcheck_uri(&self, mut endpoint: String) -> crate::Result<Uri> {
        if !endpoint.ends_with('/') {
            endpoint.push('/');
        }
        let url = format!("{}api/v1/ping", endpoint);
        Ok(url.parse::<Uri>().context(super::UriParseSnafu)?)
    }

    pub fn authorization(&self) -> String {
        let mut auth = String::from("Basic ");
        let user_pwd = format!("{}:{}", self.user, self.password);
        let encoded = base64::engine::general_purpose::STANDARD_NO_PAD.encode(user_pwd);
        auth.push_str(&encoded);
        auth
    }
}

fn healthcheck(
    endpoint: String,
    settings: CnosDBSettings,
    mut client: HttpClient,
) -> crate::Result<super::Healthcheck> {
    let uri = settings.healthcheck_uri(endpoint)?;

    let request = hyper::Request::get(uri).body(hyper::Body::empty()).unwrap();

    Ok(async move {
        client
            .call(request)
            .await
            .map_err(|error| error.into())
            .and_then(|response| match response.status() {
                StatusCode::OK => Ok(()),
                other => Err(super::HealthcheckError::UnexpectedStatus { status: other }.into()),
            })
    }
    .boxed())
}

fn get_ts_from_value(value: Option<Value>) -> i64 {
    match value {
        Some(Value::Timestamp(ts)) => ts.timestamp_nanos(),
        _ => Utc::now().timestamp_nanos(),
    }
}

fn value_to_line_string(value: &Value) -> String {
    match value {
        Value::Bytes(bytes) => {
            let mut res = String::from_utf8_lossy(bytes)
                .into_owned()
                .replace('"', "\\\"");
            res.insert(0, '"');
            res.push('"');
            res
        }
        Value::Timestamp(ts) => ts.timestamp_nanos().to_string() + "i",
        Value::Integer(i) => i.to_string() + "i",
        _ => value.to_string_lossy().into_owned(),
    }
}

fn build_line_protocol(
    table: &str,
    tags: HashMap<String, String>,
    fields: HashMap<String, String>,
    timestamp: i64,
) -> String {
    let mut output = String::new();

    output.push_str(table);
    output.push(',');

    for (i, (key, value)) in tags.iter().enumerate() {
        if i > 0 {
            output.push(',');
        }
        output.push_str(key);
        output.push('=');
        output.push_str(value);
    }

    output.push(' ');

    for (i, (key, value)) in fields.into_iter().enumerate() {
        if i > 0 {
            output.push(',');
        }
        output.push_str(&key);
        output.push('=');
        output.push_str(&value);
    }

    output.push(' ');
    output.push_str(&timestamp.to_string());
    output.push('\n');

    output
}

#[cfg(test)]
mod test {
    use crate::sinks::cnosdb::CnosDBSettings;
    use bytes::Bytes;
    use chrono::DateTime;
    use ordered_float::NotNan;
    use rand_distr::num_traits::FromPrimitive;
    use std::collections::HashMap;
    use vrl::value::Value;

    #[test]
    fn test_cnosdb_sink_build_line_protocol() {
        let table = "test";
        let tags = vec![("tag1", "value1")];
        let fields = vec![("field1", "value1")];
        let timestamp = 1234567890;

        let mut tags_map = HashMap::new();
        for (key, value) in tags {
            tags_map.insert(key.to_string(), value.to_string());
        }

        let mut fields_map = HashMap::new();
        for (key, value) in fields {
            fields_map.insert(key.to_string(), value.to_string());
        }

        let output = super::build_line_protocol(table, tags_map, fields_map, timestamp);

        assert_eq!(output, "test,tag1=value1 field1=value1 1234567890\n");
    }

    #[test]
    fn test_cnosdb_sink_value_to_line_string() {
        let value_bytes = Value::Bytes(Bytes::from("test"));
        let value_timestamp = Value::Timestamp(DateTime::from_utc(
            chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
            chrono::Utc,
        ));
        let value_integer = Value::Integer(1234567890);
        let value_float = Value::Float(NotNan::from_f64(1_234_567_890.123_456_7).unwrap());
        let value_bool = Value::Boolean(true);
        assert_eq!(super::value_to_line_string(&value_bytes), "\"test\"");
        assert_eq!(super::value_to_line_string(&value_timestamp), "0i");
        assert_eq!(super::value_to_line_string(&value_integer), "1234567890i");
        assert_eq!(
            super::value_to_line_string(&value_float),
            "1234567890.1234567"
        );
        assert_eq!(super::value_to_line_string(&value_bool), "true");
    }

    #[test]
    fn test_cnosdb_sink_write_uri() {
        let settings = CnosDBSettings {
            user: "user".to_string(),
            password: "password".to_string(),
            tenant: "tenant".to_string(),
            database: "database".to_string(),
        };
        let endpoint = "http://localhost:8902/".to_string();
        let uri = settings.write_uri(endpoint).unwrap();
        assert_eq!(
            uri.to_string(),
            "http://localhost:8902/api/v1/write?db=database&tenant=tenant"
        );
    }

    #[test]
    fn test_cnosdb_sink_health_check_uri() {
        let settings = CnosDBSettings {
            user: "user".to_string(),
            password: "password".to_string(),
            tenant: "tenant".to_string(),
            database: "database".to_string(),
        };
        let endpoint = "http://localhost:8902/".to_string();
        let uri = settings.healthcheck_uri(endpoint).unwrap();
        assert_eq!(uri.to_string(), "http://localhost:8902/api/v1/ping");
    }

    #[test]
    fn test_cnosdb_sink_authorization() {
        let settings = CnosDBSettings {
            user: "user".to_string(),
            password: "password".to_string(),
            tenant: "tenant".to_string(),
            database: "database".to_string(),
        };
        let authorization = settings.authorization();
        assert_eq!(authorization, "Basic dXNlcjpwYXNzd29yZA")
    }
}
