use std::time::Duration;

use futures::{FutureExt, StreamExt};
use http::Uri;
use hyper::{Body, Request};
use serde_with::serde_as;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::config::LogNamespace;
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol,
};
use vector_lib::EstimatedJsonEncodedSizeOf;

use self::types::Stats;
use crate::{
    config::{SourceConfig, SourceContext, SourceOutput},
    http::HttpClient,
    internal_events::{
        EventStoreDbMetricsHttpError, EventStoreDbStatsParsingError, EventsReceived,
        StreamClosedError,
    },
    tls::TlsSettings,
};

pub mod types;

/// Configuration for the `eventstoredb_metrics` source.
#[serde_as]
#[configurable_component(source(
    "eventstoredb_metrics",
    "Receive metrics from collected by a EventStoreDB."
))]
#[derive(Clone, Debug, Default)]
pub struct EventStoreDbConfig {
    /// Endpoint to scrape stats from.
    #[serde(default = "default_endpoint")]
    #[configurable(metadata(docs::examples = "https://localhost:2113/stats"))]
    endpoint: String,

    /// The interval between scrapes, in seconds.
    #[serde(default = "default_scrape_interval_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    scrape_interval_secs: Duration,

    /// Overrides the default namespace for the metrics emitted by the source.
    ///
    /// By default, `eventstoredb` is used.
    #[configurable(metadata(docs::examples = "eventstoredb"))]
    default_namespace: Option<String>,
}

const fn default_scrape_interval_secs() -> Duration {
    Duration::from_secs(15)
}

pub fn default_endpoint() -> String {
    "https://localhost:2113/stats".to_string()
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

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

fn eventstoredb(
    endpoint: String,
    interval: Duration,
    namespace: Option<String>,
    mut cx: SourceContext,
) -> crate::Result<super::Source> {
    let mut ticks = IntervalStream::new(tokio::time::interval(interval)).take_until(cx.shutdown);
    let tls_settings = TlsSettings::from_options(&None)?;
    let client = HttpClient::new(tls_settings, &cx.proxy)?;
    let url: Uri = endpoint.as_str().parse()?;

    let bytes_received = register!(BytesReceived::from(Protocol::HTTP));
    let events_received = register!(EventsReceived);

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
                        bytes_received.emit(ByteSize(bytes.len()));

                        match serde_json::from_slice::<Stats>(bytes.as_ref()) {
                            Err(error) => {
                                emit!(EventStoreDbStatsParsingError { error });
                                continue;
                            }

                            Ok(stats) => {
                                let metrics = stats.metrics(namespace.clone());
                                let count = metrics.len();
                                let byte_size = metrics.estimated_json_encoded_size_of();

                                events_received.emit(CountByteSize(count, byte_size));

                                if (cx.out.send_batch(metrics).await).is_err() {
                                    emit!(StreamClosedError { count });
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

    const EVENTSTOREDB_SCRAPE_ADDRESS: &str = "http://eventstoredb:2113/stats";

    #[tokio::test]
    async fn scrape_something() {
        let config = EventStoreDbConfig {
            endpoint: EVENTSTOREDB_SCRAPE_ADDRESS.to_owned(),
            scrape_interval_secs: Duration::from_secs(1),
            default_namespace: None,
        };

        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(5), &SOURCE_TAGS).await;
        assert!(!events.is_empty());
    }
}
