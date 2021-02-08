use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    http::HttpClient,
    sinks::{
        util::{
            batch::{Batch, BatchError},
            encode_event,
            encoding::{EncodingConfig, EncodingConfiguration},
            http::{BatchedHttpSink, HttpSink},
            BatchConfig, BatchSettings, BoxedRawValue, Compression, Encoding, JsonArrayBuffer,
            TowerRequestConfig, VecBuffer,
        },
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsConfig},
};
use bytes::Bytes;
use flate2::write::GzEncoder;
use futures::{FutureExt, SinkExt};
use http::{Request, StatusCode};
use hyper::body::Body;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{io::Write, time::Duration};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DatadogLogsConfig {
    endpoint: Option<String>,
    region: Option<super::Region>,
    api_key: String,
    encoding: EncodingConfig<Encoding>,
    tls: Option<TlsConfig>,

    #[serde(default)]
    compression: Option<Compression>,

    #[serde(default)]
    batch: BatchConfig,

    #[serde(default)]
    request: TowerRequestConfig,
}

#[derive(Clone)]
pub struct DatadogLogsJsonService {
    config: DatadogLogsConfig,
}

#[derive(Clone)]
pub struct DatadogLogsTextService {
    config: DatadogLogsConfig,
}

inventory::submit! {
    SinkDescription::new::<DatadogLogsConfig>("datadog_logs")
}

impl GenerateConfig for DatadogLogsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"api_key = "${DATADOG_API_KEY_ENV_VAR}"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

impl DatadogLogsConfig {
    fn get_endpoint(&self) -> &str {
        self.endpoint
            .as_deref()
            .unwrap_or_else(|| match self.region {
                Some(super::Region::Eu) => "https://http-intake.logs.datadoghq.eu",
                None | Some(super::Region::Us) => "https://http-intake.logs.datadoghq.com",
            })
    }

    fn batch_settings<T: Batch>(&self) -> Result<BatchSettings<T>, BatchError> {
        BatchSettings::default()
            .bytes(bytesize::kib(100u64))
            .events(20)
            .timeout(1)
            .parse_config(self.batch)
    }

    /// Builds the required BatchedHttpSink.
    /// Since the DataDog sink can create one of two different sinks, this
    /// extracts most of the shared functionality required to create either sink.
    fn build_sink<T, B, O>(
        &self,
        cx: SinkContext,
        service: T,
        batch: B,
        timeout: Duration,
    ) -> crate::Result<(VectorSink, Healthcheck)>
    where
        O: 'static,
        B: Batch<Output = Vec<O>> + std::marker::Send + 'static,
        B::Output: std::marker::Send + Clone,
        B::Input: std::marker::Send,
        T: HttpSink<Input = B::Input, Output = B::Output> + Clone,
    {
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());

        let tls_settings = MaybeTlsSettings::from_config(
            &Some(self.tls.clone().unwrap_or_else(TlsConfig::enabled)),
            false,
        )?;

        let client = HttpClient::new(tls_settings)?;
        let healthcheck = healthcheck(service.clone(), client.clone()).boxed();
        let sink = BatchedHttpSink::new(
            service,
            batch,
            request_settings,
            timeout,
            client,
            cx.acker(),
        )
        .sink_map_err(|error| error!(message = "Fatal datadog_logs text sink error.", %error));

        Ok((VectorSink::Sink(Box::new(sink)), healthcheck))
    }

    /// Build the request, GZipping the contents if the config specifies.
    fn build_request(
        &self,
        content_type: &str,
        body: Vec<u8>,
    ) -> crate::Result<http::Request<Vec<u8>>> {
        let uri = format!("{}/v1/input", self.get_endpoint());
        let request = Request::post(uri)
            .header("Content-Type", content_type)
            .header("DD-API-KEY", self.api_key.clone());

        let compression = self.compression.unwrap_or(Compression::Gzip(None));

        let (request, body) = match compression {
            Compression::None => (request, body),
            Compression::Gzip(level) => {
                // Default the compression level to 6, which is similar to datadog agent.
                // https://docs.datadoghq.com/agent/logs/log_transport/?tab=https#log-compression
                let level = level.unwrap_or(6);
                let mut encoder =
                    GzEncoder::new(Vec::new(), flate2::Compression::new(level as u32));

                encoder.write_all(&body)?;
                (
                    request.header("Content-Encoding", "gzip"),
                    encoder.finish()?,
                )
            }
        };

        request
            .header("Content-Length", body.len())
            .body(body)
            .map_err(Into::into)
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_logs")]
impl SinkConfig for DatadogLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        // Create a different sink depending on which encoding we have chosen.
        // Json and Text have different batching strategies and so each needs to be
        // handled differently.
        match self.encoding.codec {
            Encoding::Json => {
                let batch_settings = self.batch_settings()?;
                self.build_sink(
                    cx,
                    DatadogLogsJsonService {
                        config: self.clone(),
                    },
                    JsonArrayBuffer::new(batch_settings.size),
                    batch_settings.timeout,
                )
            }
            Encoding::Text => {
                let batch_settings = self.batch_settings()?;
                self.build_sink(
                    cx,
                    DatadogLogsTextService {
                        config: self.clone(),
                    },
                    VecBuffer::new(batch_settings.size),
                    batch_settings.timeout,
                )
            }
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "datadog_logs"
    }
}

#[async_trait::async_trait]
impl HttpSink for DatadogLogsJsonService {
    type Input = serde_json::Value;
    type Output = Vec<BoxedRawValue>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        let log = event.as_mut_log();

        if let Some(message) = log.remove(log_schema().message_key()) {
            log.insert("message", message);
        }

        if let Some(timestamp) = log.remove(log_schema().timestamp_key()) {
            log.insert("date", timestamp);
        }

        if let Some(host) = log.remove(log_schema().host_key()) {
            log.insert("host", host);
        }

        self.config.encoding.apply_rules(&mut event);

        Some(json!(event.into_log()))
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let body = serde_json::to_vec(&events)?;
        self.config.build_request("application/json", body)
    }
}

#[async_trait::async_trait]
impl HttpSink for DatadogLogsTextService {
    type Input = Bytes;
    type Output = Vec<Bytes>;

    fn encode_event(&self, event: Event) -> Option<Self::Input> {
        encode_event(event, &self.config.encoding)
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let body: Vec<u8> = events.into_iter().flat_map(Bytes::into_iter).collect();
        self.config.build_request("text/plain", body)
    }
}

/// The healthcheck is performed by sending an empty request to Datadog and checking
/// the return.
async fn healthcheck<T, O>(sink: T, client: HttpClient) -> crate::Result<()>
where
    T: HttpSink<Output = Vec<O>>,
{
    let req = sink.build_request(Vec::new()).await?.map(Body::from);

    let res = client.send(req).await?;

    let status = res.status();
    let body = hyper::body::to_bytes(res.into_body()).await?;

    match status {
        StatusCode::OK => Ok(()),
        StatusCode::UNAUTHORIZED => {
            let json: serde_json::Value = serde_json::from_slice(&body[..])?;

            Err(json
                .as_object()
                .and_then(|o| o.get("error"))
                .and_then(|s| s.as_str())
                .unwrap_or("Token is not valid, 401 returned.")
                .to_string()
                .into())
        }
        _ => {
            let body = String::from_utf8_lossy(&body[..]);

            Err(format!(
                "Server returned unexpected error status: {} body: {}",
                status, body
            )
            .into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::SinkConfig,
        sinks::util::test::{build_test_server, load_sink},
        test_util::{next_addr, random_lines_with_stream},
    };
    use futures::StreamExt;
    use pretty_assertions::assert_eq;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogLogsConfig>();
    }

    #[tokio::test]
    async fn smoke_text() {
        let (mut config, cx) = load_sink::<DatadogLogsConfig>(
            r#"
            api_key = "atoken"
            encoding = "text"
            compression = "none"
            batch.max_events = 1
            "#,
        )
        .unwrap();

        let addr = next_addr();
        // Swap out the endpoint so we can force send it
        // to our local server
        let endpoint = format!("http://{}", addr);
        config.endpoint = Some(endpoint.clone());

        let (sink, _) = config.build(cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        let (expected, events) = random_lines_with_stream(100, 10);

        let _ = sink.run(events).await.unwrap();

        let output = rx.take(expected.len()).collect::<Vec<_>>().await;

        for (i, val) in output.iter().enumerate() {
            assert_eq!(val.0.headers.get("Content-Type").unwrap(), "text/plain");
            assert_eq!(val.1, format!("{}\n", expected[i]));
        }
    }

    #[tokio::test]
    async fn smoke_json() {
        let (mut config, cx) = load_sink::<DatadogLogsConfig>(
            r#"
            api_key = "atoken"
            encoding = "json"
            compression = "none"
            batch.max_events = 1
            "#,
        )
        .unwrap();

        let addr = next_addr();
        // Swap out the endpoint so we can force send it
        // to our local server
        let endpoint = format!("http://{}", addr);
        config.endpoint = Some(endpoint.clone());

        let (sink, _) = config.build(cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        let (expected, events) = random_lines_with_stream(100, 10);

        let _ = sink.run(events).await.unwrap();

        let output = rx.take(expected.len()).collect::<Vec<_>>().await;

        for (i, val) in output.iter().enumerate() {
            assert_eq!(
                val.0.headers.get("Content-Type").unwrap(),
                "application/json"
            );

            let mut json = serde_json::Deserializer::from_slice(&val.1[..])
                .into_iter::<serde_json::Value>()
                .map(|v| v.expect("decoding json"));

            let json = json.next().unwrap();

            // The json we send to Datadog is an array of events.
            // As we have set batch.max_events to 1, each entry will be
            // an array containing a single record.
            let message = json
                .get(0)
                .unwrap()
                .get("message")
                .unwrap()
                .as_str()
                .unwrap();
            assert_eq!(message, expected[i]);
        }
    }
}
