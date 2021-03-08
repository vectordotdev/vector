use self::types::Stats;
use crate::internal_events::{
    EventStoreDbMetricsHttpError, EventStoreDbMetricsReceived, EventStoreDbStatsParsingError,
};
use crate::{
    config::{self, SourceConfig, SourceContext, SourceDescription},
    Event,
};
use futures::{stream, FutureExt, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_stream::wrappers::IntervalStream;

pub mod types;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
struct EventStoreDbConfig {
    #[serde(default = "default_endpoint")]
    endpoint: String,
    #[serde(default = "default_scrape_interval_secs")]
    scrape_interval_secs: u64,
    default_namespace: Option<String>,
}

pub fn default_scrape_interval_secs() -> u64 {
    3
}

pub fn default_endpoint() -> String {
    "https://localhost:2113/".to_string()
}

inventory::submit! {
    SourceDescription::new::<EventStoreDbConfig>("eventstoredb_metrics")
}

impl_generate_config_from_default!(EventStoreDbConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "eventstoredb_metrics")]
impl SourceConfig for EventStoreDbConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        eventstoredb(
            self.endpoint.as_str(),
            self.scrape_interval_secs,
            self.default_namespace.clone(),
            cx,
        )
    }

    fn output_type(&self) -> config::DataType {
        config::DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "eventstoredb_metrics"
    }
}

fn eventstoredb(
    endpoint: &str,
    interval: u64,
    namespace: Option<String>,
    cx: SourceContext,
) -> crate::Result<super::Source> {
    let mut out = cx
        .out
        .sink_map_err(|error| error!(message = "Error sending metric.", %error));
    let mut ticks = IntervalStream::new(tokio::time::interval(Duration::from_secs(interval)))
        .take_until(cx.shutdown);
    let client = hyper::Client::builder().build(hyper_openssl::HttpsConnector::new()?);
    let url: http::Uri = format!("{}/stats", endpoint).parse()?;

    Ok(Box::pin(
        async move {
            while ticks.next().await.is_some() {
                let req = hyper::Request::get(&url)
                    .header("content-type", "application/json")
                    .body(hyper::Body::empty())
                    .unwrap();

                match client.request(req).await {
                    Err(error) => {
                        emit!(EventStoreDbMetricsHttpError {
                            error: error.into(),
                        });
                        continue;
                    }

                    Ok(resp) => {
                        let bytes = match hyper::body::to_bytes(resp.into_body()).await {
                            Ok(b) => b,
                            Err(error) => {
                                emit!(EventStoreDbMetricsHttpError {
                                    error: error.into(),
                                });
                                continue;
                            }
                        };

                        match serde_json::from_slice::<Stats>(bytes.as_ref()) {
                            Err(error) => {
                                emit!(EventStoreDbStatsParsingError { error });
                                continue;
                            }

                            Ok(stats) => {
                                let metrics = stats.metrics(namespace.clone());
                                let mut metrics = stream::iter(metrics).map(Event::Metric).map(Ok);

                                emit!(EventStoreDbMetricsReceived {
                                    byte_size: bytes.len(),
                                });

                                if out.send_all(&mut metrics).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        .map(Ok)
        .boxed(),
    ))
}
