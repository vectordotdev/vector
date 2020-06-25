use crate::{
    sinks::http::{HttpMethod, HttpSinkConfig},
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        service2::TowerRequestConfig,
        BatchBytesConfig, Compression,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use http::Uri;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use snafu::Snafu;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display(
        "Missing authentication key, must provide either 'license_key' or 'insert_key'"
    ))]
    MissingAuthParam,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum NewRelicLogsRegion {
    #[derivative(Default)]
    Us,
    Eu,
}

#[derive(Deserialize, Serialize, Debug, Derivative, Clone)]
#[derivative(Default)]
pub struct NewRelicLogsConfig {
    pub license_key: Option<String>,
    pub insert_key: Option<String>,
    pub region: Option<NewRelicLogsRegion>,
    #[serde(skip_serializing_if = "skip_serializing_if_default", default)]
    pub encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchBytesConfig,

    #[serde(default)]
    pub request: TowerRequestConfig,
}

inventory::submit! {
    SinkDescription::new::<NewRelicLogsConfig>("new_relic_logs")
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Json,
}

impl From<Encoding> for crate::sinks::http::Encoding {
    fn from(v: Encoding) -> crate::sinks::http::Encoding {
        match v {
            Encoding::Json => crate::sinks::http::Encoding::Json,
        }
    }
}

// There is another one of these in `util::encoding`, but this one is specialized for New Relic.
/// For encodings, answers "Is it possible to skip serializing this value, because it's the
/// default?"
pub(crate) fn skip_serializing_if_default(e: &EncodingConfigWithDefault<Encoding>) -> bool {
    e.codec() == &Encoding::default()
}

#[typetag::serde(name = "new_relic_logs")]
impl SinkConfig for NewRelicLogsConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let http_conf = self.create_config()?;
        http_conf.build(cx)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "new_relic_logs"
    }
}

impl NewRelicLogsConfig {
    fn create_config(&self) -> crate::Result<HttpSinkConfig> {
        let mut headers: IndexMap<String, String> = IndexMap::new();

        if let Some(license_key) = &self.license_key {
            headers.insert("X-License-Key".to_owned(), license_key.clone());
        } else if let Some(insert_key) = &self.insert_key {
            headers.insert("X-Insert-Key".to_owned(), insert_key.clone());
        } else {
            return Err(Box::new(BuildError::MissingAuthParam));
        }

        let uri = match self.region.as_ref().unwrap_or(&NewRelicLogsRegion::Us) {
            NewRelicLogsRegion::Us => Uri::from_static("https://log-api.newrelic.com/log/v1"),
            NewRelicLogsRegion::Eu => Uri::from_static("https://log-api.eu.newrelic.com/log/v1"),
        };

        let batch = BatchBytesConfig {
            // The max request size is 10MiB, so in order to be comfortably
            // within this we batch up to 5MiB.
            max_size: Some(self.batch.max_size.unwrap_or(bytesize::mib(5u64) as usize)),
            ..self.batch
        };

        let request = TowerRequestConfig {
            // The default throughput ceiling defaults are relatively
            // conservative so we crank them up for New Relic.
            in_flight_limit: Some(self.request.in_flight_limit.unwrap_or(100)),
            rate_limit_num: Some(self.request.rate_limit_num.unwrap_or(100)),
            ..self.request
        };

        Ok(HttpSinkConfig {
            uri: uri.into(),
            method: Some(HttpMethod::Post),
            healthcheck_uri: None,
            auth: None,
            headers: Some(headers),
            compression: self.compression,
            encoding: self.encoding.clone().without_default(),

            batch,
            request,

            tls: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::Event,
        sinks::util::test::build_test_server,
        test_util::{next_addr, runtime, shutdown_on_idle},
        topology::config::SinkConfig,
    };
    use futures01::{stream, Sink, Stream};
    use hyper::Method;
    use serde_json::Value;
    use std::io::BufRead;

    #[test]
    fn new_relic_logs_check_config_no_auth() {
        assert_eq!(
            format!(
                "{}",
                NewRelicLogsConfig::default().create_config().unwrap_err()
            ),
            "Missing authentication key, must provide either 'license_key' or 'insert_key'"
                .to_owned(),
        );
    }

    #[test]
    fn new_relic_logs_check_config_defaults() {
        let mut nr_config = NewRelicLogsConfig::default();
        nr_config.license_key = Some("foo".to_owned());
        let http_config = nr_config.create_config().unwrap();

        assert_eq!(
            format!("{}", http_config.uri),
            "https://log-api.newrelic.com/log/v1".to_string()
        );
        assert_eq!(http_config.method, Some(HttpMethod::Post));
        assert_eq!(http_config.encoding.codec(), &Encoding::Json.into());
        assert_eq!(
            http_config.batch.max_size,
            Some(bytesize::mib(5u64) as usize)
        );
        assert_eq!(http_config.request.in_flight_limit, Some(100));
        assert_eq!(http_config.request.rate_limit_num, Some(100));
        assert_eq!(
            http_config.headers.unwrap()["X-License-Key"],
            "foo".to_owned()
        );
        assert!(http_config.tls.is_none());
        assert!(http_config.auth.is_none());
    }

    #[test]
    fn new_relic_logs_check_config_custom() {
        let mut nr_config = NewRelicLogsConfig::default();
        nr_config.insert_key = Some("foo".to_owned());
        nr_config.region = Some(NewRelicLogsRegion::Eu);
        nr_config.batch.max_size = Some(bytesize::mib(8u64) as usize);
        nr_config.request.in_flight_limit = Some(12);
        nr_config.request.rate_limit_num = Some(24);

        let http_config = nr_config.create_config().unwrap();

        assert_eq!(
            format!("{}", http_config.uri),
            "https://log-api.eu.newrelic.com/log/v1".to_string()
        );
        assert_eq!(http_config.method, Some(HttpMethod::Post));
        assert_eq!(http_config.encoding.codec(), &Encoding::Json.into());
        assert_eq!(
            http_config.batch.max_size,
            Some(bytesize::mib(8u64) as usize)
        );
        assert_eq!(http_config.request.in_flight_limit, Some(12));
        assert_eq!(http_config.request.rate_limit_num, Some(24));
        assert_eq!(
            http_config.headers.unwrap()["X-Insert-Key"],
            "foo".to_owned()
        );
        assert!(http_config.tls.is_none());
        assert!(http_config.auth.is_none());
    }

    #[test]
    fn new_relic_logs_check_config_custom_from_toml() {
        let config = r#"
        insert_key = "foo"
        region = "eu"

        [batch]
        max_size = 8388608

        [request]
        in_flight_limit = 12
        rate_limit_num = 24
    "#;
        let nr_config: NewRelicLogsConfig = toml::from_str(&config).unwrap();

        let http_config = nr_config.create_config().unwrap();

        assert_eq!(
            format!("{}", http_config.uri),
            "https://log-api.eu.newrelic.com/log/v1".to_string()
        );
        assert_eq!(http_config.method, Some(HttpMethod::Post));
        assert_eq!(http_config.encoding.codec(), &Encoding::Json.into());
        assert_eq!(
            http_config.batch.max_size,
            Some(bytesize::mib(8u64) as usize)
        );
        assert_eq!(http_config.request.in_flight_limit, Some(12));
        assert_eq!(http_config.request.rate_limit_num, Some(24));
        assert_eq!(
            http_config.headers.unwrap()["X-Insert-Key"],
            "foo".to_owned()
        );
        assert!(http_config.tls.is_none());
        assert!(http_config.auth.is_none());
    }

    #[test]
    fn new_relic_logs_happy_path() {
        let in_addr = next_addr();

        let mut nr_config = NewRelicLogsConfig::default();
        nr_config.license_key = Some("foo".to_owned());
        let mut http_config = nr_config.create_config().unwrap();
        http_config.uri = format!("http://{}/fake_nr", in_addr)
            .parse::<http::Uri>()
            .unwrap()
            .into();

        let mut rt = runtime();

        let (sink, _healthcheck) = http_config
            .build(SinkContext::new_test(rt.executor()))
            .unwrap();
        let (rx, trigger, server) = build_test_server(in_addr, &mut rt);

        let input_lines = (0..100).map(|i| format!("msg {}", i)).collect::<Vec<_>>();
        let events = stream::iter_ok(input_lines.clone().into_iter().map(Event::from));

        let pump = sink.send_all(events);

        rt.spawn(server);

        let _ = rt.block_on(pump).unwrap();
        drop(trigger);

        let output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!(Method::POST, parts.method);
                assert_eq!("/fake_nr", parts.uri.path());
                assert_eq!(
                    parts
                        .headers
                        .get("X-License-Key")
                        .and_then(|v| v.to_str().ok()),
                    Some("foo")
                );
                body
            })
            .map(std::io::Cursor::new)
            .flat_map(BufRead::lines)
            .map(Result::unwrap)
            .flat_map(|s| -> Vec<String> {
                let vals: Vec<Value> = serde_json::from_str(&s).unwrap();
                vals.iter()
                    .map(|v| v.get("message").unwrap().as_str().unwrap().to_owned())
                    .collect()
            })
            .collect::<Vec<_>>();

        shutdown_on_idle(rt);

        assert_eq!(input_lines, output_lines);
    }
}
