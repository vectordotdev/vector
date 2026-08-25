use std::{collections::HashMap, time::Duration};

use bytes::Bytes;
use futures_util::FutureExt;
use http::{Uri, response::Parts};
use serde_with::serde_as;
use snafu::ResultExt;
use vector_lib::{config::LogNamespace, configurable::configurable_component, event::Event};

use super::parser;
use crate::{
    Result,
    config::{GenerateConfig, SourceConfig, SourceContext, SourceOutput},
    http::{Auth, QueryParameters},
    internal_events::PrometheusParseError,
    sources::{
        self,
        util::{
            http::HttpMethod,
            http_client::{
                GenericHttpClientInputs, HttpClientBuilder, HttpClientContext, build_url, call,
                default_interval, default_timeout, warn_if_interval_too_low,
            },
        },
    },
    tls::{TlsConfig, TlsSettings},
};

// pulled up, and split over multiple lines, because the long lines trip up rustfmt such that it
// gave up trying to format, but reported no error
static PARSE_ERROR_NO_PATH: &str = "No path is set on the endpoint and we got a parse error,\
                                    did you mean to use /metrics? This behavior changed in version 0.11.";
static NOT_FOUND_NO_PATH: &str = "No path is set on the endpoint and we got a 404,\
                                  did you mean to use /metrics?\
                                  This behavior changed in version 0.11.";

/// Configuration for the `prometheus_scrape` source.
#[serde_as]
#[configurable_component(source(
    "prometheus_scrape",
    "Collect metrics from Prometheus exporters."
))]
#[derive(Clone, Debug)]
pub struct PrometheusScrapeConfig {
    /// Endpoints to scrape metrics from.
    #[configurable(metadata(docs::examples = "http://localhost:9090/metrics"))]
    #[serde(alias = "hosts")]
    endpoints: Vec<String>,

    /// The interval between scrapes. Requests are run concurrently so if a scrape takes longer
    /// than the interval a new scrape will be started. This can take extra resources, set the timeout
    /// to a value lower than the scrape interval to prevent this from happening.
    #[serde(default = "default_interval")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(rename = "scrape_interval_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    interval: Duration,

    /// The timeout for each scrape request.
    #[serde(default = "default_timeout")]
    #[serde_as(as = "serde_with:: DurationSecondsWithFrac<f64>")]
    #[serde(rename = "scrape_timeout_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Timeout"))]
    timeout: Duration,

    /// The tag name added to each event representing the scraped instance's `host:port`.
    ///
    /// The tag value is the host and port of the scraped instance.
    instance_tag: Option<String>,

    /// The tag name added to each event representing the scraped instance's endpoint.
    ///
    /// The tag value is the endpoint of the scraped instance.
    endpoint_tag: Option<String>,

    /// Controls how tag conflicts are handled if the scraped source has tags to be added.
    ///
    /// If `true`, the new tag is not added if the scraped metric has the tag already. If `false`, the conflicting tag
    /// is renamed by prepending `exported_` to the original name.
    ///
    /// This matches Prometheus’ `honor_labels` configuration.
    #[serde(default = "crate::serde::default_false")]
    honor_labels: bool,

    /// Custom parameters for the scrape request query string.
    ///
    /// One or more values for the same parameter key can be provided. The parameters provided in this option are
    /// appended to any parameters manually provided in the `endpoints` option. This option is especially useful when
    /// scraping the `/federate` endpoint.
    #[serde(default)]
    #[configurable(metadata(docs::additional_props_description = "A query string parameter."))]
    #[configurable(metadata(docs::examples = "query_example()"))]
    query: QueryParameters,

    #[configurable(derived)]
    tls: Option<TlsConfig>,

    #[configurable(derived)]
    auth: Option<Auth>,
}

fn query_example() -> serde_json::Value {
    serde_json::json! ({
        "match[]": [
            "{job=\"somejob\"}",
            "{__name__=~\"job:.*\"}"
        ]
    })
}

impl GenerateConfig for PrometheusScrapeConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            endpoints: vec!["http://localhost:9090/metrics".to_string()],
            interval: default_interval(),
            timeout: default_timeout(),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: false,
            query: HashMap::new(),
            tls: None,
            auth: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_scrape")]
impl SourceConfig for PrometheusScrapeConfig {
    async fn build(&self, cx: SourceContext) -> Result<sources::Source> {
        let urls = self
            .endpoints
            .iter()
            .map(|s| s.parse::<Uri>().context(sources::UriParseSnafu))
            .map(|r| r.map(|uri| build_url(&uri, &self.query)))
            .collect::<std::result::Result<Vec<Uri>, sources::BuildError>>()?;
        let tls = TlsSettings::from_options(self.tls.as_ref())?;

        let builder = PrometheusScrapeBuilder {
            honor_labels: self.honor_labels,
            instance_tag: self.instance_tag.clone(),
            endpoint_tag: self.endpoint_tag.clone(),
        };

        warn_if_interval_too_low(self.timeout, self.interval);

        let inputs = GenericHttpClientInputs {
            urls,
            interval: self.interval,
            timeout: self.timeout,
            headers: HashMap::new(),
            content_type: "text/plain".to_string(),
            auth: self.auth.clone(),
            tls,
            proxy: cx.proxy.clone(),
            shutdown: cx.shutdown,
        };

        Ok(call(inputs, builder, cx.out, HttpMethod::Get).boxed())
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

// InstanceInfo stores the scraped instance info and the tag to insert into the log event with. It
// is used to join these two pieces of info to avoid storing the instance if instance_tag is not
// configured
#[derive(Clone)]
struct InstanceInfo {
    tag: String,
    instance: String,
    honor_label: bool,
}

// EndpointInfo stores the scraped endpoint info and the tag to insert into the log event with. It
// is used to join these two pieces of info to avoid storing the endpoint if endpoint_tag is not
// configured
#[derive(Clone)]
struct EndpointInfo {
    tag: String,
    endpoint: String,
    honor_label: bool,
}

/// Captures the configuration options required to build request-specific context.
#[derive(Clone)]
struct PrometheusScrapeBuilder {
    honor_labels: bool,
    instance_tag: Option<String>,
    endpoint_tag: Option<String>,
}

impl HttpClientBuilder for PrometheusScrapeBuilder {
    type Context = PrometheusScrapeContext;

    /// Expands the context with the instance info and endpoint info for the current request.
    fn build(&self, url: &Uri) -> Self::Context {
        let instance_info = self.instance_tag.as_ref().map(|tag| {
            let instance = format!(
                "{}:{}",
                url.host().unwrap_or_default(),
                url.port_u16().unwrap_or_else(|| match url.scheme() {
                    Some(scheme) if scheme == &http::uri::Scheme::HTTP => 80,
                    Some(scheme) if scheme == &http::uri::Scheme::HTTPS => 443,
                    _ => 0,
                })
            );
            InstanceInfo {
                tag: tag.to_string(),
                instance,
                honor_label: self.honor_labels,
            }
        });
        let endpoint_info = self.endpoint_tag.as_ref().map(|tag| EndpointInfo {
            tag: tag.to_string(),
            endpoint: url.to_string(),
            honor_label: self.honor_labels,
        });
        PrometheusScrapeContext {
            instance_info,
            endpoint_info,
        }
    }
}

/// Request-specific context required for decoding into events.
struct PrometheusScrapeContext {
    instance_info: Option<InstanceInfo>,
    endpoint_info: Option<EndpointInfo>,
}

impl HttpClientContext for PrometheusScrapeContext {
    fn enrich_events(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            let metric = event.as_mut_metric();
            if let Some(InstanceInfo {
                tag,
                instance,
                honor_label,
            }) = &self.instance_info
            {
                match (honor_label, metric.tag_value(tag)) {
                    (false, Some(old_instance)) => {
                        metric.replace_tag(format!("exported_{tag}"), old_instance);
                        metric.replace_tag(tag.clone(), instance.clone());
                    }
                    (true, Some(_)) => {}
                    (_, None) => {
                        metric.replace_tag(tag.clone(), instance.clone());
                    }
                }
            }
            if let Some(EndpointInfo {
                tag,
                endpoint,
                honor_label,
            }) = &self.endpoint_info
            {
                match (honor_label, metric.tag_value(tag)) {
                    (false, Some(old_endpoint)) => {
                        metric.replace_tag(format!("exported_{tag}"), old_endpoint);
                        metric.replace_tag(tag.clone(), endpoint.clone());
                    }
                    (true, Some(_)) => {}
                    (_, None) => {
                        metric.replace_tag(tag.clone(), endpoint.clone());
                    }
                }
            }
        }
    }

    /// Parses the Prometheus HTTP response into metric events
    fn on_response(&mut self, url: &Uri, _header: &Parts, body: &Bytes) -> Option<Vec<Event>> {
        let body = String::from_utf8_lossy(body);

        match parser::parse_text(&body) {
            Ok(events) => Some(events),
            Err(error) => {
                if url.path() == "/" {
                    // https://github.com/vectordotdev/vector/pull/3801#issuecomment-700723178
                    warn!(
                        message = PARSE_ERROR_NO_PATH,
                        endpoint = %url,
                    );
                }
                emit!(PrometheusParseError {
                    error,
                    url: url.clone(),
                    body,
                });
                None
            }
        }
    }

    fn on_http_response_error(&self, url: &Uri, header: &Parts) {
        if header.status == hyper::StatusCode::NOT_FOUND && url.path() == "/" {
            // https://github.com/vectordotdev/vector/pull/3801#issuecomment-700723178
            warn!(
                message = NOT_FOUND_NO_PATH,
                endpoint = %url,
            );
        }
    }
}

#[cfg(all(test, feature = "sinks-prometheus"))]
mod test;

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests;
