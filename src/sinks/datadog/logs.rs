use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    sinks::{
        util::{
            encode_event,
            encoding::{EncodingConfig, EncodingConfiguration},
            http::{BatchedHttpSink, HttpClient, HttpSink},
            BatchConfig, BatchSettings, BoxedRawValue, Compression, Encoding, JsonArrayBuffer,
            TowerRequestConfig, VecBuffer,
        },
        Healthcheck, VectorSink,
    },
};
use bytes::Bytes;
use flate2::write::GzEncoder;
use futures::FutureExt;
use futures01::Sink;
use http::{Request, StatusCode};
use hyper::body::Body;
use serde::{Deserialize, Serialize};
use serde_json::json;
use string_cache::DefaultAtom as Atom;
use std::io::Write;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DatadogLogsConfig {
    endpoint: Option<String>,
    api_key: String,
    encoding: EncodingConfig<Encoding>,

    #[serde(default)]
    compression: Compression,

    compression_level: Option<u32>,

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

impl GenerateConfig for DatadogLogsConfig {}

impl DatadogLogsConfig {
    fn get_endpoint(&self) -> &str {
        match &self.endpoint {
            Some(ref endpoint) => &endpoint,
            None => "https://http-intake.logs.datadoghq.eu/v1/input",
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_logs")]
impl SinkConfig for DatadogLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());

        let client = HttpClient::new(cx.resolver(), None)?;

        // Create a different sink depending on which encoding we have chosen.
        // Json and text have different batching strategies and so each needs to be
        // handled differently.
        match self.encoding.codec {
            Encoding::Json => {
                let batch_settings = BatchSettings::default()
                    .bytes(bytesize::kib(100u64))
                    .timeout(1)
                    .parse_config(self.batch)?;

                let service = DatadogLogsJsonService {
                    config: self.clone(),
                };
                let healthcheck = healthcheck(service.clone(), client.clone()).boxed();
                let sink = BatchedHttpSink::new(
                    service,
                    JsonArrayBuffer::new(batch_settings.size),
                    request_settings,
                    batch_settings.timeout,
                    client,
                    cx.acker(),
                )
                .sink_map_err(|e| error!("Fatal datadog_logs json sink error: {}", e));

                Ok((VectorSink::Futures01Sink(Box::new(sink)), healthcheck))
            }
            Encoding::Text => {
                let batch_settings = BatchSettings::default()
                    .bytes(bytesize::kib(100u64))
                    .timeout(1)
                    .parse_config(self.batch)?;

                let service = DatadogLogsTextService {
                    config: self.clone(),
                };
                let healthcheck = healthcheck(service.clone(), client.clone()).boxed();
                let sink = BatchedHttpSink::new(
                    service,
                    VecBuffer::new(batch_settings.size),
                    request_settings,
                    batch_settings.timeout,
                    client,
                    cx.acker(),
                )
                .sink_map_err(|e| error!("Fatal datadog_logs text sink error: {}", e));

                Ok((VectorSink::Futures01Sink(Box::new(sink)), healthcheck))
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

        if let Some(message) = log.remove(&Atom::from(log_schema().message_key())) {
            log.insert("message", message);
        }

        if let Some(timestamp) = log.remove(&Atom::from(log_schema().timestamp_key())) {
            log.insert("date", timestamp);
        }

        if let Some(host) = log.remove(&Atom::from(log_schema().host_key())) {
            log.insert("host", host);
        }

        self.config.encoding.apply_rules(&mut event);

        Some(json!(event.into_log()))
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let uri = self.config.get_endpoint();

        let request = Request::post(uri)
            .header("Content-Type", "application/json")
            .header("DD-API-KEY", self.config.api_key.clone());

        let body = serde_json::to_vec(&events)?;
        build_request(&self.config, body)
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
        let body: Vec<u8> = events.iter().flat_map(|b| b.into_iter()).cloned().collect();

        build_request(&self.config, body)
    }
}

/// Build the request, GZipping the contents if the config specifies.
fn build_request(
    config: &DatadogLogsConfig,
    body: Vec<u8>,
) -> crate::Result<http::Request<Vec<u8>>> {
    let uri = config.get_endpoint();
    let request = Request::post(uri)
        .header("Content-Type", "text/plain")
        .header("DD-API-KEY", config.api_key.clone());

    let (request, body) = match config.compression {
        Compression::None => (request, body),
        Compression::Gzip => {
            let mut encoder = GzEncoder::new(
                Vec::new(),
                match config.compression_level {
                    Some(level) if level <= 9 => flate2::Compression::new(level),
                    _ => flate2::Compression::fast(),
                },
            );

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

/// The healthcheck is performed by sending an empty request to Datadog and checking
/// the return.
async fn healthcheck<T, O>(config: T, mut client: HttpClient) -> crate::Result<()>
where
    T: HttpSink<Output = Vec<O>>,
{
    let req = config.build_request(Vec::new()).await?.map(Body::from);

    let res = client.send(req).await?;

    let status = res.status();
    let body = hyper::body::to_bytes(res.into_body()).await?;

    if status == StatusCode::OK {
        Ok(())
    } else if status == StatusCode::UNAUTHORIZED {
        let json: serde_json::Value = serde_json::from_slice(&body[..])?;

        let message = if let Some(s) = json
            .as_object()
            .and_then(|o| o.get("error"))
            .and_then(|s| s.as_str())
        {
            s.to_string()
        } else {
            "Token is not valid, 401 returned.".to_string()
        };

        Err(message.into())
    } else {
        let body = String::from_utf8_lossy(&body[..]);

        Err(format!(
            "Server returned unexpected error status: {} body: {}",
            status, body
        )
        .into())
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

        let _ = config.build(cx.clone()).unwrap();

        let addr = next_addr();
        // Swap out the endpoint so we can force send it
        // to our local server
        let endpoint = format!("http://{}", addr);
        config.endpoint = Some(endpoint.clone());

        let (sink, _) = config.build(cx).unwrap();

        let (rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        let (expected, events) = random_lines_with_stream(100, 10);

        let _ = sink.run(events).await.unwrap();

        let output = rx.take(expected.len()).collect::<Vec<_>>().await;

        for (i, val) in output.iter().enumerate() {
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

        let _ = config.build(cx.clone()).unwrap();

        let addr = next_addr();
        // Swap out the endpoint so we can force send it
        // to our local server
        let endpoint = format!("http://{}", addr);
        config.endpoint = Some(endpoint.clone());

        let (sink, _) = config.build(cx).unwrap();

        let (rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        let (expected, events) = random_lines_with_stream(100, 10);

        let _ = sink.run(events).await.unwrap();

        let output = rx.take(expected.len()).collect::<Vec<_>>().await;

        for (i, val) in output.iter().enumerate() {
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
