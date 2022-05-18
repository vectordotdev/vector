use std::time::Duration;

use futures::{FutureExt, StreamExt};
use http::Uri;
use hyper::{Body, Request};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::IntervalStream;
use vector_core::ByteSizeOf;

use self::types::Stats;
use crate::{
    config::{self, Output, SourceConfig, SourceContext, SourceDescription},
    http::HttpClient,
    internal_events::{
        BytesReceived, EventStoreDbMetricsHttpError, EventStoreDbStatsParsingError,
        OldEventsReceived, StreamClosedError,
    },
    tls::TlsSettings,
};

pub mod types;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
struct EventStoreDbConfig {
    #[serde(default = "default_endpoint")]
    endpoint: String,
    #[serde(default = "default_scrape_interval_secs")]
    scrape_interval_secs: u64,
    default_namespace: Option<String>,
}

const fn default_scrape_interval_secs() -> u64 {
    15
}

pub fn default_endpoint() -> String {
    "https://localhost:2113/stats".to_string()
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
            self.endpoint.clone(),
            self.scrape_interval_secs,
            self.default_namespace.clone(),
            cx,
        )
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(config::DataType::Metric)]
    }

    fn source_type(&self) -> &'static str {
        "eventstoredb_metrics"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

fn eventstoredb(
    endpoint: String,
    interval: u64,
    namespace: Option<String>,
    mut cx: SourceContext,
) -> crate::Result<super::Source> {
    let mut ticks = IntervalStream::new(tokio::time::interval(Duration::from_secs(interval)))
        .take_until(cx.shutdown);
    let tls_settings = TlsSettings::from_options(&None)?;
    let client = HttpClient::new(tls_settings, &cx.proxy)?;
    let url: Uri = endpoint.as_str().parse()?;

    Ok(Box::pin(
        async move {
            while ticks.next().await.is_some() {
                let req = Request::get(&url)
                    .header("content-type", "application/json")
                    .body(Body::empty())
                    .expect("Building request should be infallible.");

                match client.send(req).await {
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
                        emit!(BytesReceived {
                            byte_size: bytes.len(),
                            protocol: "http",
                        });

                        match serde_json::from_slice::<Stats>(bytes.as_ref()) {
                            Err(error) => {
                                emit!(EventStoreDbStatsParsingError { error });
                                continue;
                            }

                            Ok(stats) => {
                                let metrics = stats.metrics(namespace.clone());
                                let count = metrics.len();
                                let byte_size = metrics.size_of();

                                emit!(OldEventsReceived { count, byte_size });

                                if let Err(error) = cx.out.send_batch(metrics).await {
                                    emit!(StreamClosedError { count, error });
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

#[cfg(all(test, feature = "eventstoredb_metrics-integration-tests"))]
mod integration_tests {
    use tokio::time::Duration;

    use super::*;
    use crate::test_util::components::{run_and_assert_source_compliance, SOURCE_TAGS};

    const EVENTSTOREDB_SCRAPE_ADDRESS: &str = "http://localhost:2113/stats";

    #[tokio::test]
    async fn scrape_something() {
        let config = EventStoreDbConfig {
            endpoint: EVENTSTOREDB_SCRAPE_ADDRESS.to_owned(),
            scrape_interval_secs: 1,
            default_namespace: None,
        };

        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(5), &SOURCE_TAGS).await;
        assert!(!events.is_empty());
    }
}
