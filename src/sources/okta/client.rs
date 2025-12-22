use std::{sync::Arc, time::Duration};

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures::StreamExt as _;
use futures_util::{FutureExt, Stream, stream};
use http::Uri;
use hyper::{Body, Request};
use percent_encoding::utf8_percent_encode;
use serde_with::serde_as;
use tokio::sync::Mutex;
use tokio_stream::wrappers::IntervalStream;
use tokio_util::codec::Decoder as _;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{
        JsonDeserializerConfig, StreamDecodingError,
        decoding::{DeserializerConfig, FramingConfig},
    },
    config::{LogNamespace, SourceOutput, proxy::ProxyConfig},
    configurable::configurable_component,
    event::Event,
    json_size::JsonSize,
    shutdown::ShutdownSignal,
    tls::TlsConfig,
};

use crate::{
    SourceSender,
    codecs::{Decoder, DecodingConfig},
    config::{SourceConfig, SourceContext},
    http::{HttpClient, HttpError},
    internal_events::{
        DecoderDeserializeError, EndpointBytesReceived, HttpClientEventsReceived,
        HttpClientHttpError, HttpClientHttpResponseError, StreamClosedError,
    },
    sources,
    sources::util::http_client::{default_interval, default_timeout, warn_if_interval_too_low},
    tls::TlsSettings,
};

/// Configuration for the `okta` source.
#[serde_as]
#[configurable_component(source("okta", "Pull Okta system logs via the Okta API",))]
#[derive(Clone, Debug)]
pub struct OktaConfig {
    /// The Okta subdomain to scrape
    #[configurable(metadata(docs::examples = "foo.okta.com"))]
    pub domain: String,

    /// API token for authentication
    #[configurable(metadata(docs::examples = "00xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"))]
    pub token: String,

    /// The interval between scrapes. Requests are run concurrently so if a scrape takes longer
    /// than the interval, a new scrape will be started. This can take extra resources, set the timeout
    /// to a value lower than the scrape interval to prevent this from happening.
    #[serde(default = "default_interval")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(rename = "scrape_interval_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    pub interval: Duration,

    /// The timeout for each scrape request.
    #[serde(default = "default_timeout")]
    #[serde_as(as = "serde_with::DurationSecondsWithFrac<f64>")]
    #[serde(rename = "scrape_timeout_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Timeout"))]
    pub timeout: Duration,

    /// The time to look back for logs. This is used to determine the start time of the first request
    /// (that is, the earliest log to fetch)
    #[configurable(metadata(docs::human_name = "Since (seconds before now)"))]
    pub since: Option<u64>,

    /// TLS configuration.
    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

impl Default for OktaConfig {
    fn default() -> Self {
        Self {
            domain: "".to_string(),
            token: "".to_string(),
            interval: default_interval(),
            timeout: default_timeout(),
            since: None,
            tls: None,
            log_namespace: None,
        }
    }
}

impl_generate_config_from_default!(OktaConfig);

fn find_rel_next_link(header: &str) -> Option<String> {
    for part in header.split(',') {
        let relpart: Vec<_> = part.split(';').collect();
        if let Some(url) = relpart
            .first()
            .map(|s| s.trim().trim_matches(|c| c == '<' || c == '>'))
            && part.contains("rel=\"next\"")
        {
            return Some(url.to_string());
        }
    }
    None
}

#[async_trait::async_trait]
#[typetag::serde(name = "okta")]
impl SourceConfig for OktaConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let since = match self.since {
            Some(since) => Utc::now() - Duration::from_secs(since),
            _ => Utc::now(),
        };

        let path_and_query = format!(
            "/api/v1/logs?since={}",
            utf8_percent_encode(&since.to_rfc3339(), percent_encoding::NON_ALPHANUMERIC)
        );

        let mut url_parts = Uri::try_from(&self.domain)
            .map_err(|_| {
                format!(
                    "Invalid domain: {}. Must be a valid Okta subdomain.",
                    self.domain
                )
            })?
            .into_parts();

        url_parts.path_and_query = Some(path_and_query.parse()?);
        if url_parts.scheme.is_none() {
            url_parts.scheme = Some(http::uri::Scheme::HTTPS);
        }

        let url = Uri::from_parts(url_parts).map_err(|_| {
            format!(
                "Invalid domain: {}. Must be a valid Okta subdomain.",
                self.domain
            )
        })?;

        let tls = TlsSettings::from_options(self.tls.as_ref())?;

        let log_namespace = cx.log_namespace(self.log_namespace);

        warn_if_interval_too_low(self.timeout, self.interval);

        Ok(run(
            url,
            tls,
            cx.proxy,
            self.token.clone(),
            self.interval,
            self.timeout,
            log_namespace,
            cx.shutdown,
            cx.out,
        )
        .boxed())
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        // There is a global and per-source `log_namespace` config. The source config overrides the global setting,
        // and is merged here.
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        vec![SourceOutput::new_maybe_logs(
            JsonDeserializerConfig::default().output_type(),
            JsonDeserializerConfig::default().schema_definition(log_namespace),
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

fn enrich_events(events: &mut Vec<Event>, log_namespace: LogNamespace) {
    let now = Utc::now();
    for event in events {
        log_namespace.insert_standard_vector_source_metadata(
            event.as_mut_log(),
            OktaConfig::NAME,
            now,
        );
    }
}

type OktaRunResult =
    Result<(http::response::Parts, Bytes, Option<Uri>), Box<dyn std::error::Error + Send + Sync>>;

type OktaTimeoutResult =
    Result<Result<http::Response<Body>, HttpError>, tokio::time::error::Elapsed>;

async fn run_once(url: String, result: OktaTimeoutResult, timeout: Duration) -> OktaRunResult {
    let mut next: Option<Uri> = None;
    match result {
        Ok(Ok(response)) => {
            let (header, body) = response.into_parts();
            if let Some(next_url) = header
                .headers
                .get_all("link")
                .iter()
                .filter_map(|v| v.to_str().ok())
                .filter_map(find_rel_next_link)
                .next()
                .and_then(|next| Uri::try_from(next).ok())
            {
                next = Some(next_url);
            };

            let body = http_body::Body::collect(body).await?.to_bytes();

            emit!(EndpointBytesReceived {
                byte_size: body.len(),
                protocol: "http",
                endpoint: &url,
            });
            Ok((header, body, next))
        }
        Ok(Err(error)) => Err(error.into()),
        Err(_) => Err(format!("Timeout error: request exceeded {}s", timeout.as_secs_f64()).into()),
    }
}

fn handle_response(
    response: OktaRunResult,
    decoder: Decoder,
    log_namespace: LogNamespace,
    url: String,
) -> Option<impl Stream<Item = Event> + Send + use<>> {
    match response {
        Ok((header, body, _)) if header.status == hyper::StatusCode::OK => {
            let mut buf = BytesMut::new();
            buf.extend_from_slice(&body);
            let mut events = decode_events(&mut buf, decoder);
            let byte_size = if events.is_empty() {
                JsonSize::zero()
            } else {
                events.estimated_json_encoded_size_of()
            };

            emit!(HttpClientEventsReceived {
                byte_size,
                count: events.len(),
                url,
            });

            if events.is_empty() {
                return None;
            }

            enrich_events(&mut events, log_namespace);

            Some(stream::iter(events))
        }
        Ok((header, _, _)) => {
            emit!(HttpClientHttpResponseError {
                code: header.status,
                url,
            });
            None
        }
        Err(error) => {
            emit!(HttpClientHttpError { error, url });
            None
        }
    }
}

/// Calls the Okta system logs API and sends the events to the output stream.
///
/// Okta's API paginates with a `link` header that contains a url (in `rel=next`) to the next page of results,
/// and will always return a `rel=next` link regardless of whether there are more results.
/// This function fetches all pages until there are no more results (an empty JSON array) and finishes until
/// the next interval
/// The function will run until the `shutdown` signal is received.
#[allow(clippy::too_many_arguments)] // internal function
async fn run(
    url: Uri,
    tls: TlsSettings,
    proxy: ProxyConfig,
    token: String,
    interval: Duration,
    timeout: Duration,
    log_namespace: LogNamespace,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let url_mutex = Arc::new(Mutex::new(url.clone()));
    let decoder = DecodingConfig::new(
        FramingConfig::Bytes,
        DeserializerConfig::Json(JsonDeserializerConfig::default()),
        log_namespace,
    )
    .build()
    .map_err(|ref e| {
        emit!(DecoderDeserializeError { error: e });
    })?;

    let client = HttpClient::new(tls, &proxy).map_err(|e| {
        emit!(HttpClientHttpError {
            error: Box::new(e),
            url: url.to_string()
        });
    })?;

    let mut stream = IntervalStream::new(tokio::time::interval(interval))
        .take_until(shutdown)
        .then(move |_| {
            let client = client.clone();
            let url_mutex = Arc::clone(&url_mutex);
            let token = token.clone();
            let decoder = decoder.clone();

            async move {
                stream::unfold((), move |_| {
                    let url_mutex = Arc::clone(&url_mutex);
                    let token = token.clone();
                    let decoder = decoder.clone();
                    let client = client.clone();

                    async move {
                        let (run_url, response): (String, OktaRunResult) = {
                            // We update the actual URL based on the response the API returns
                            // so the critical section is between here & when the request finishes
                            let mut url_lock = url_mutex.lock().await;
                            let url = url_lock.to_string();

                            let mut request = match Request::get(&url).body(Body::empty()) {
                                Ok(request) => request,
                                Err(e) => {
                                    emit!(HttpClientHttpError {
                                        error: e.into(),
                                        url: url.clone(),
                                    });
                                    return None;
                                }
                            };

                            let headers = request.headers_mut();
                            headers.insert(
                                http::header::AUTHORIZATION,
                                format!("SSWS {token}").parse().unwrap(),
                            );
                            headers
                                .insert(http::header::ACCEPT, "application/json".parse().unwrap());
                            headers.insert(
                                http::header::CONTENT_TYPE,
                                "application/json".parse().unwrap(),
                            );

                            let client = client.clone();
                            let response = tokio::time::timeout(timeout, client.send(request))
                                .then({
                                    let url = url.clone();
                                    move |result| run_once(url, result, timeout)
                                })
                                .await;

                            if let Ok((_, _, Some(ref next))) = response {
                                *url_lock = next.clone();
                            }
                            let new_url = url_lock.to_string();

                            (new_url, response)
                        };

                        handle_response(response, decoder, log_namespace, run_url)
                            .map(|events| (events, ()))
                    }
                })
                .flatten()
                .boxed()
            }
        })
        .flatten_unordered(None)
        .boxed();

    match out.send_event_stream(&mut stream).await {
        Ok(()) => {
            debug!("Finished sending.");
            Ok(())
        }
        Err(_) => {
            let (count, _) = stream.size_hint();
            emit!(StreamClosedError { count });
            Err(())
        }
    }
}

fn decode_events(buf: &mut BytesMut, mut decoder: Decoder) -> Vec<Event> {
    let mut events = Vec::new();
    loop {
        match decoder.decode_eof(buf) {
            Ok(Some((next, _))) => {
                events.extend(next);
            }
            Ok(None) => break,
            Err(error) => {
                // Error is logged by `crate::codecs::Decoder`, no further
                // handling is needed here.
                if !error.can_continue() {
                    break;
                }
                break;
            }
        }
    }
    events
}
