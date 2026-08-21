//! Common logic for sources that are HTTP clients.
//!
//! Specific HTTP client sources will:
//!   - Call build_url() to build the URL(s) to call.
//!   - Implement a specific context struct which:
//!       - Contains the data that source needs in order to process the HTTP responses into internal_events
//!       - Implements the HttpClient trait
//!   - Call call() supplying the generic inputs for calling and the source-specific
//!     context.

// Okta source only imports defaults but doesn't use the rest of the client
#![cfg_attr(feature = "sources-okta", allow(dead_code))]

use std::{collections::HashMap, future::ready, time::Duration};

use bytes::Bytes;
use futures_util::{FutureExt, StreamExt, TryFutureExt, stream};
use http::{Uri, response::Parts};
use hyper::{Body, Request};
use tokio_stream::wrappers::IntervalStream;
use vector_lib::{
    EstimatedJsonEncodedSizeOf, config::proxy::ProxyConfig, event::Event, json_size::JsonSize,
    shutdown::ShutdownSignal,
};

use crate::{
    SourceSender,
    http::{Auth, HttpClient, QueryParameterValue, QueryParameters},
    internal_events::{
        EndpointBytesReceived, HttpClientEventsReceived, HttpClientHttpError,
        HttpClientHttpResponseError, StreamClosedError,
    },
    sources::util::http::HttpMethod,
    tls::TlsSettings,
};

/// Contains the inputs generic to any http client.
pub(crate) struct GenericHttpClientInputs {
    /// Array of URLs to call.
    pub urls: Vec<Uri>,
    /// Interval between calls.
    pub interval: Duration,
    /// Delay before the first call.
    pub initial_delay: Duration,
    /// When set, each call happens at a position inside its own interval rather than on a fixed
    /// cadence, and intervals that go by while the stream is not polled are skipped rather than
    /// replayed. Different seeds normally produce different deterministic sequences, reducing
    /// persistent alignment between components without guaranteeing that individual calls never
    /// coincide.
    ///
    /// `None` keeps `tokio::time::interval` unchanged: a fixed cadence whose missed ticks fire
    /// back to back once polling resumes.
    pub jitter_seed: Option<String>,
    /// Timeout for the HTTP request.
    pub timeout: Duration,
    /// Map of Header+Value to apply to HTTP request.
    pub headers: HashMap<String, Vec<String>>,
    /// Content type of the HTTP request, determined by the source.
    pub content_type: String,
    pub auth: Option<Auth>,
    pub tls: TlsSettings,
    pub proxy: ProxyConfig,
    pub shutdown: ShutdownSignal,
}

/// The default interval to call the HTTP endpoint if none is configured.
pub(crate) const fn default_interval() -> Duration {
    Duration::from_secs(15)
}

/// The default timeout for the HTTP request if none is configured.
pub(crate) const fn default_timeout() -> Duration {
    Duration::from_secs(5)
}

/// Picks the stream that decides when each call happens.
///
/// Only a caller that asked for jitter gets [`schedule`]. Everything else keeps
/// `tokio::time::interval` verbatim, including its `MissedTickBehavior::Burst` catch-up after a
/// stall, so sources that do not opt in behave exactly as they did before jitter existed.
///
/// The jitter window is one whole interval, which makes the jittered calls land across the interval
/// rather than near one preferred position. This reduces persistent alignment with periodic work,
/// but does not guarantee that individual calls from different components never coincide.
fn ticks(
    start: tokio::time::Instant,
    interval: Duration,
    jitter_seed: Option<String>,
) -> futures_util::stream::BoxStream<'static, ()> {
    match jitter_seed {
        Some(seed) => schedule(start, interval, interval, seed).boxed(),
        None => IntervalStream::new(tokio::time::interval_at(start, interval))
            .map(|_| ())
            .boxed(),
    }
}

/// Builds the jittered stream that decides when each call happens.
///
/// Call `n` fires at `start + n * interval`, pushed forward by a jitter of up to `jitter_window`.
/// Every call is anchored to that ideal grid rather than sleeping for `interval + jitter` after the
/// previous one, so the jitter cannot accumulate: with a `jitter_window` of one interval, call `n`
/// always lands in `[start + n * interval, start + (n + 1) * interval)`. Under normal polling this
/// schedules one call in each interval, while the gap between two consecutive calls can fall
/// anywhere in `(0, 2 * interval)`.
///
/// Grid points that go by without the stream being polled are dropped rather than fired late one
/// after another, so a stall never turns into a burst of catch-up calls. This differs from
/// `tokio::time::interval`, which defaults to `MissedTickBehavior::Burst` and does replay them;
/// callers that need the `tokio` semantics must not use this schedule.
///
/// A zero `jitter_window` reproduces a plain fixed-cadence interval, still without the catch-up
/// burst.
fn schedule(
    start: tokio::time::Instant,
    interval: Duration,
    jitter_window: Duration,
    seed: String,
) -> impl futures_util::Stream<Item = ()> + Send {
    assert!(!interval.is_zero(), "`interval` must be non-zero.");

    stream::unfold((start, 0u64), move |state| {
        // Time can pass between two polls without any call happening: the previous call can run
        // long, the runtime can be busy, or the pipeline downstream can be applying backpressure.
        // Drop the grid points that went by rather than firing one call for each of them, which
        // would turn a stall into exactly the burst this schedule exists to avoid.
        let (grid, tick) = skip_missed(state, interval);
        let deadline = grid + window_offset(&format!("{seed}\0{tick}"), jitter_window);
        let next = (grid + interval, tick.wrapping_add(1));

        async move {
            tokio::time::sleep_until(deadline).await;
            Some(((), next))
        }
    })
}

/// Moves a grid point forward past the calls that were missed while nothing polled the schedule.
///
/// At most one grid point is left behind the current instant, so recovering from a stall costs a
/// single call that fires straight away instead of one call per interval that went by.
fn skip_missed(
    (mut grid, mut tick): (tokio::time::Instant, u64),
    interval: Duration,
) -> (tokio::time::Instant, u64) {
    if interval.is_zero() {
        return (grid, tick);
    }

    let now = tokio::time::Instant::now();
    while now.saturating_duration_since(grid) >= interval {
        grid += interval;
        tick = tick.wrapping_add(1);
    }

    (grid, tick)
}

/// Maps `seed` onto a position in the range `[0, window)`.
///
/// The position comes from a hash instead of a random number, which spreads different seeds
/// approximately uniformly across the window while keeping the result reproducible: the same seed
/// always lands in the same place, so a schedule built on this can be replayed and tested.
///
/// Callers are expected to seed this with the host name, the component ID and the call number. The
/// component ID gives components in one Vector instance different sequences, the host name does the
/// same for instances with different host names, and the call number keeps a component from
/// settling onto one position. Hash-derived positions can still coincide, and instances with the
/// same host name and component ID follow the same sequence.
fn window_offset(seed: &str, window: Duration) -> Duration {
    let window_ms = u64::try_from(window.as_millis()).unwrap_or(u64::MAX);
    if window_ms == 0 {
        return Duration::ZERO;
    }

    Duration::from_millis(seahash::hash(seed.as_bytes()) % window_ms)
}

/// Builds the context, allowing the source-specific implementation to leverage data from the
/// config and the current HTTP request.
pub(crate) trait HttpClientBuilder {
    type Context: HttpClientContext;

    /// Called before the HTTP request is made to build out the context.
    fn build(&self, url: &Uri) -> Self::Context;
}

/// Methods that allow context-specific behavior during the scraping procedure.
pub(crate) trait HttpClientContext {
    /// Called after the HTTP request succeeds and returns the decoded/parsed Event array.
    fn on_response(&mut self, url: &Uri, header: &Parts, body: &Bytes) -> Option<Vec<Event>>;

    /// (Optional) Called if the HTTP response is not 200 ('OK').
    fn on_http_response_error(&self, _uri: &Uri, _header: &Parts) {}

    /// (Optional) Process the base URL before each request.
    /// Allows for dynamic query parameters that update at runtime.
    /// Returns a new URL if parameters need to be updated, or None to use the original URL.
    fn process_url(&self, _url: &Uri) -> Option<Uri> {
        None
    }

    /// (Optional) Get the request body to send with the HTTP request.
    /// Returns the body as a String if one should be sent, or None for an empty body.
    fn get_request_body(&self) -> Option<String> {
        None
    }

    // This function can be defined to enrich events with additional HTTP
    // metadata. This function should be used rather than internal enrichment so
    // that accurate byte count metrics can be emitted.
    fn enrich_events(&mut self, _events: &mut Vec<Event>) {}
}

/// Builds a url for the HTTP requests.
pub(crate) fn build_url(uri: &Uri, query: &QueryParameters) -> Uri {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    if let Some(query) = uri.query() {
        serializer.extend_pairs(url::form_urlencoded::parse(query.as_bytes()));
    };
    for (k, query_value) in query {
        match query_value {
            QueryParameterValue::SingleParam(param) => {
                serializer.append_pair(k, param.value());
            }
            QueryParameterValue::MultiParams(params) => {
                for v in params {
                    serializer.append_pair(k, v.value());
                }
            }
        };
    }
    let mut builder = Uri::builder();
    if let Some(scheme) = uri.scheme() {
        builder = builder.scheme(scheme.clone());
    };
    if let Some(authority) = uri.authority() {
        builder = builder.authority(authority.clone());
    };
    builder = builder.path_and_query(match serializer.finish() {
        query if !query.is_empty() => format!("{}?{}", uri.path(), query),
        _ => uri.path().to_string(),
    });
    builder
        .build()
        .expect("Failed to build URI from parsed arguments")
}

/// Warns if the scrape timeout is greater than the scrape interval.
pub(crate) fn warn_if_interval_too_low(timeout: Duration, interval: Duration) {
    if timeout > interval {
        warn!(
            interval_secs = %interval.as_secs_f64(),
            timeout_secs = %timeout.as_secs_f64(),
            message = "Having a scrape timeout that exceeds the scrape interval can lead to excessive resource consumption.",
        );
    }
}

/// Calls one or more urls at an interval.
///   - The HTTP request is built per the options in provided generic inputs.
///   - The HTTP response is decoded/parsed into events by the specific context.
///   - The events are then sent to the output stream.
pub(crate) async fn call<
    B: HttpClientBuilder<Context = C> + Send + Clone,
    C: HttpClientContext + Send,
>(
    inputs: GenericHttpClientInputs,
    context_builder: B,
    mut out: SourceSender,
    http_method: HttpMethod,
) -> Result<(), ()> {
    // Building the HttpClient should not fail as it is just setting up the client with the
    // proxy and tls settings.
    let client =
        HttpClient::new(inputs.tls.clone(), &inputs.proxy).expect("Building HTTP client failed");
    let start = tokio::time::Instant::now() + inputs.initial_delay;
    let mut stream = ticks(start, inputs.interval, inputs.jitter_seed)
        .take_until(inputs.shutdown)
        .map(move |_| stream::iter(inputs.urls.clone()))
        .flatten()
        .map(move |base_url| {
            let client = client.clone();
            let endpoint = base_url.to_string();

            let context_builder = context_builder.clone();
            let mut context = context_builder.build(&base_url);

            // Check if we need to process the URL dynamically (for updating VRL expressions)
            let url = context.process_url(&base_url).unwrap_or(base_url);

            let mut builder = match http_method {
                HttpMethod::Head => Request::head(&url),
                HttpMethod::Get => Request::get(&url),
                HttpMethod::Post => Request::post(&url),
                HttpMethod::Put => Request::put(&url),
                HttpMethod::Patch => Request::patch(&url),
                HttpMethod::Delete => Request::delete(&url),
                HttpMethod::Options => Request::options(&url),
            };

            // add user specified headers
            for (header, values) in &inputs.headers {
                for value in values {
                    builder = builder.header(header, value);
                }
            }

            // set ACCEPT header if not user specified
            if !inputs.headers.contains_key(http::header::ACCEPT.as_str()) {
                builder = builder.header(http::header::ACCEPT, &inputs.content_type);
            }

            // Get the request body from the context (if any)
            let body = match context.get_request_body() {
                Some(body_str) => {
                    // Set Content-Type header if not already set
                    if !inputs
                        .headers
                        .contains_key(http::header::CONTENT_TYPE.as_str())
                    {
                        builder = builder.header(http::header::CONTENT_TYPE, "application/json");
                    }
                    Body::from(body_str)
                }
                None => Body::empty(),
            };

            // building the request should be infallible
            let mut request = builder.body(body).expect("error creating request");

            if let Some(auth) = &inputs.auth {
                auth.apply(&mut request);
            }

            tokio::time::timeout(inputs.timeout, client.send(request))
                .then(move |result| async move {
                    match result {
                        Ok(Ok(response)) => Ok(response),
                        Ok(Err(error)) => Err(error.into()),
                        Err(_) => Err(format!(
                            "Timeout error: request exceeded {}s",
                            inputs.timeout.as_secs_f64()
                        )
                        .into()),
                    }
                })
                .and_then(|response| async move {
                    let (header, body) = response.into_parts();
                    let body = http_body::Body::collect(body).await?.to_bytes();
                    emit!(EndpointBytesReceived {
                        byte_size: body.len(),
                        protocol: "http",
                        endpoint: endpoint.as_str(),
                    });
                    Ok((header, body))
                })
                .into_stream()
                .filter_map(move |response| {
                    ready(match response {
                        Ok((header, body)) if header.status == hyper::StatusCode::OK => {
                            context.on_response(&url, &header, &body).map(|mut events| {
                                let byte_size = if events.is_empty() {
                                    // We need to explicitly set the byte size
                                    // to 0 since
                                    // `estimated_json_encoded_size_of` returns
                                    // at least 1 for an empty collection. For
                                    // the purposes of the
                                    // HttpClientEventsReceived event, we should
                                    // emit 0 when there aren't any usable
                                    // metrics.
                                    JsonSize::zero()
                                } else {
                                    events.estimated_json_encoded_size_of()
                                };

                                emit!(HttpClientEventsReceived {
                                    byte_size,
                                    count: events.len(),
                                    url: url.to_string()
                                });

                                // We'll enrich after receiving the events so
                                // that the byte sizes are accurate.
                                context.enrich_events(&mut events);

                                stream::iter(events)
                            })
                        }
                        Ok((header, _)) => {
                            context.on_http_response_error(&url, &header);
                            emit!(HttpClientHttpResponseError {
                                code: header.status,
                                url: url.to_string(),
                            });
                            None
                        }
                        Err(error) => {
                            emit!(HttpClientHttpError {
                                error,
                                url: url.to_string()
                            });
                            None
                        }
                    })
                })
                .flatten()
                .boxed()
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use futures_util::StreamExt;
    use tokio::time::{Duration, Instant};

    use super::{schedule, ticks, window_offset};

    /// Collects when each of the first `count` calls fires, relative to `Instant::now()`.
    async fn tick_offsets(
        interval: Duration,
        jitter_window: Duration,
        count: usize,
    ) -> Vec<Duration> {
        let started_at = Instant::now();
        let mut ticks = Box::pin(schedule(
            started_at,
            interval,
            jitter_window,
            "some-seed".to_owned(),
        ));
        let mut offsets = Vec::with_capacity(count);

        for _ in 0..count {
            ticks.next().await.unwrap();
            offsets.push(Instant::now() - started_at);
        }

        offsets
    }

    /// Counts how many calls fire without any time passing, i.e. the catch-up burst.
    async fn immediate_ticks(stream: &mut futures_util::stream::BoxStream<'static, ()>) -> usize {
        let mut immediate = 0;
        loop {
            let before = Instant::now();
            stream.next().await.unwrap();
            if Instant::now() != before {
                return immediate;
            }
            immediate += 1;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn ticks_without_a_seed_keep_the_tokio_catch_up_burst() {
        let interval = Duration::from_secs(10);
        let mut stream = ticks(Instant::now(), interval, None);

        stream.next().await.unwrap();

        // Nothing polls for five intervals, as happens under downstream backpressure.
        tokio::time::advance(interval * 5 + Duration::from_secs(1)).await;

        // `tokio::time::interval` defaults to `MissedTickBehavior::Burst`, so every missed tick is
        // replayed back to back. Sources that did not opt into jitter must keep seeing exactly
        // that: this is the behavior that shipped before the jitter option existed.
        assert_eq!(immediate_ticks(&mut stream).await, 5);
    }

    #[tokio::test(start_paused = true)]
    async fn ticks_with_a_seed_skip_missed_calls() {
        let interval = Duration::from_secs(10);
        let mut stream = ticks(Instant::now(), interval, Some("some-seed".to_owned()));

        stream.next().await.unwrap();
        tokio::time::advance(interval * 5 + Duration::from_secs(1)).await;

        // The jittered schedule drops the grid points that went by, so recovering costs the single
        // call that is due now rather than one call per interval missed.
        assert_eq!(immediate_ticks(&mut stream).await, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn schedule_without_jitter_keeps_a_fixed_cadence() {
        let offsets = tick_offsets(Duration::from_secs(10), Duration::ZERO, 4).await;

        assert_eq!(
            offsets,
            vec![
                Duration::ZERO,
                Duration::from_secs(10),
                Duration::from_secs(20),
                Duration::from_secs(30),
            ]
        );
    }

    #[test]
    #[should_panic(expected = "`interval` must be non-zero.")]
    fn schedule_rejects_a_zero_interval() {
        let _schedule = schedule(
            Instant::now(),
            Duration::ZERO,
            Duration::ZERO,
            String::new(),
        );
    }

    #[tokio::test(start_paused = true)]
    async fn schedule_starts_from_the_given_instant() {
        let started_at = Instant::now();
        let mut ticks = Box::pin(schedule(
            started_at + Duration::from_secs(3),
            Duration::from_secs(10),
            Duration::ZERO,
            "some-seed".to_owned(),
        ));

        ticks.next().await.unwrap();
        assert_eq!(Instant::now() - started_at, Duration::from_secs(3));

        ticks.next().await.unwrap();
        assert_eq!(Instant::now() - started_at, Duration::from_secs(13));
    }

    #[tokio::test(start_paused = true)]
    async fn schedule_varies_the_gap_between_calls() {
        let interval = Duration::from_secs(10);
        let offsets = tick_offsets(interval, interval, 12).await;

        let gaps = offsets
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .collect::<Vec<_>>();

        // The point of the jitter: consecutive calls must not settle onto one fixed gap.
        assert!(
            gaps.iter().collect::<HashSet<_>>().len() > 1,
            "expected the gaps between calls to vary, got {gaps:?}"
        );

        // Anchoring to the grid bounds the gap at twice the interval, which is what stops the
        // jitter from turning into unbounded drift.
        for gap in &gaps {
            assert!(
                *gap < interval * 2,
                "expected every gap to stay under {:?}, got {gap:?}",
                interval * 2
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn schedule_does_not_let_jitter_accumulate() {
        let interval = Duration::from_secs(10);
        let window = interval;
        let offsets = tick_offsets(interval, window, 50).await;

        // Each call is anchored to `n * interval` rather than to the previous call, so the schedule
        // cannot drift: with uninterrupted polling, after 50 calls every one of them is still
        // inside its own interval, giving one call per interval and an average period of
        // `interval`.
        for (n, offset) in offsets.iter().enumerate() {
            let grid = interval * n as u32;

            assert!(
                *offset >= grid && *offset < grid + window,
                "call {n} should have fired in [{grid:?}, {:?}), got {offset:?}",
                grid + window
            );
        }
    }

    #[test]
    fn window_offset_is_zero_without_a_window() {
        assert_eq!(window_offset("some-seed", Duration::ZERO), Duration::ZERO);
    }

    #[test]
    fn window_offset_covers_the_window_without_leaving_it() {
        let window = Duration::from_secs(60);

        // Twelve 5-second buckets. Staying inside the window is what keeps a call in its own
        // interval; covering every bucket is what stops the sources on a host from bunching up in
        // one part of the interval, which is the whole point of picking a position at all.
        let mut occupied_buckets = [false; 12];

        for i in 0..1_000 {
            let offset = window_offset(&format!("seed-{i}"), window);

            assert!(offset < window, "seed-{i} landed outside the window");
            occupied_buckets[(offset.as_secs_f64() / 5.0) as usize] = true;
        }

        assert!(
            occupied_buckets.into_iter().all(|occupied| occupied),
            "expected offsets to cover the full window, got occupied buckets {occupied_buckets:?}"
        );
    }
}
