use crate::{
    config::{
        log_schema, DataType, GenerateConfig, GlobalOptions, Resource, SourceConfig,
        SourceDescription,
    },
    event::Event,
    shutdown::ShutdownSignal,
    sources::{
        self,
        util::{ErrorMessage, HttpSource, HttpSourceAuthConfig},
        http::{decode_body, Encoding},
    },
    tls::TlsConfig,
    Pipeline,
};
use bytes::Bytes;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
};

use warp::http::HeaderMap;

const API_HEADER: &str = "DD-API-KEY";
const DD_API_KEY: &str = "dd-api-key";

lazy_static! {
    static ref API_KEY_MATCHER: Regex = Regex::new(r"^/v1/input/(?P<api_key>[[:alnum:]]{32})/??").unwrap();
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DatadogLogsConfig {
    address: SocketAddr,
    tls: Option<TlsConfig>,
    auth: Option<HttpSourceAuthConfig>,
}

inventory::submit! {
    SourceDescription::new::<DatadogLogsConfig>("datadog_logs")
}

impl GenerateConfig for DatadogLogsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:8080".parse().unwrap(),
            tls: None,
            auth: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_logs")]
impl SourceConfig for DatadogLogsConfig {
    async fn build(
        &self,
        _: &str,
        _: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<sources::Source> {
        let source = DatadogLogsSource {};
        // We accept /v1/input & /v1/input/<API_KEY>
        source.run(self.address, "/v1/input", false, &self.tls, &self.auth, out, shutdown)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "datadog_logs"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }
}


#[derive(Clone, Default)]
struct DatadogLogsSource {}


impl HttpSource for DatadogLogsSource {
    fn build_event(
        &self,
        body: Bytes,
        header_map: HeaderMap,
        _query_parameters: HashMap<String, String>,
        request_path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        if body.is_empty() {
            // The datadog agent may sent empty payload as keep alive
            debug!(message = "Empty payload ignored.");
            return Ok(Vec::new());
        }

        let api_key = extract_api_key(&header_map, request_path).map(|mut k| {
            if k.len() > 5 {
                k.replace_range(0..k.len()-5, "***************************");
            }
            k
        });

        decode_body(body, Encoding::Json)
            .map(|mut events| {
                // Add source type & dd api key
                let key = log_schema().source_type_key();
                for event in events.iter_mut() {
                    let log = event.as_mut_log();
                    log.try_insert(key, Bytes::from("datadog_logs"));
                    api_key.clone().map(|k| event.as_mut_log().insert(DD_API_KEY, k));
                }
                events
            })
        
    }
}

fn extract_api_key<'a>(headers: &'a HeaderMap, path: &'a str) -> Option<String> {
    // Grab from url first
    if let Some(k) = API_KEY_MATCHER.captures(path).and_then (|cap| {
        cap.name("api_key").map(|key| key.as_str())
    }) {
        return Some(k.to_owned());
    }

    // Try from header next
    if let Some(key) = headers.get(API_HEADER) {
        return key.to_str().ok().map(str::to_owned);
    }

    None
}
