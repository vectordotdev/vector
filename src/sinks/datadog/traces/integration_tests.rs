use axum::{
    body::Body,
    extract::Extension,
    http::{header::CONTENT_TYPE, Request},
    routing::post,
    Router,
};
use chrono::Utc;
use flate2::read::GzDecoder;
use futures::stream;
use indoc::indoc;
use rmp_serde;
use serde::Serialize;
use std::{collections::HashMap, io::Read, net::SocketAddr, sync::Arc};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::{sleep, Duration};

use crate::{
    config::SinkConfig,
    event::Event,
    sinks::{
        datadog::traces::{
            stats::StatsPayload,
            tests::{simple_trace_event, simple_trace_event_detailed},
            DatadogTracesConfig,
        },
        util::test::load_sink,
    },
    test_util::{
        components::{assert_sink_compliance, SINK_TAGS},
        map_event_batch_stream, trace_init,
    },
};
use vector_core::event::{BatchNotifier, BatchStatus};

// The port for an http server to receive data from vector
fn vector_port() -> u16 {
    std::env::var("AGENT_TO_VECTOR_PORT")
        .unwrap_or_else(|_| "8081".to_string())
        .parse::<u16>()
        .unwrap()
}

// The port for an http server to receive data from the agent
fn agent_port() -> u16 {
    std::env::var("AGENT_ONLY_PORT")
        .unwrap_or_else(|_| "8082".to_string())
        .parse::<u16>()
        .unwrap()
}

// The agent url to post traces to.
fn trace_agent_only_url() -> String {
    std::env::var("TRACE_AGENT_ONLY_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8126/v0.3/traces".to_owned())
}

// Shared state for the HTTP server
struct AppState {
    name: String,
    tx: Sender<StatsPayload>,
}

// build the app with a route and run our app with hyper
async fn run_server(name: String, port: u16, tx: Sender<StatsPayload>) {
    let state = Arc::new(AppState {
        name: name.clone(),
        tx,
    });
    let app = Router::new()
        .route("/api/v0.2/traces", post(process_traces))
        .route("/api/v0.2/stats", post(process_stats))
        .layer(Extension(state));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    info!("HTTP server for `{}` listening on {}", name, addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// At a later time (perhaps if we make this a full e2e test), we could parse the trace payloads
// from vector and the agent and compare. As it stands, we have unit tests validating the trace
// structure for the sink so not much value in doing that yet.
async fn process_traces(Extension(_state): Extension<Arc<AppState>>, request: Request<Body>) {
    let content_type_header = request.headers().get(CONTENT_TYPE);
    let content_type = content_type_header.and_then(|value| value.to_str().ok());

    if let Some(content_type) = content_type {
        if content_type.starts_with("application/x-protobuf") {
            debug!("Got trace payload.");
        }
    }
}

// process a POST request from the stats endpoint.
// De-compresses and De-serializes the payload, then forwards it on the Sender channel.
async fn process_stats(Extension(state): Extension<Arc<AppState>>, mut request: Request<Body>) {
    debug!(
        "`{}` server process_stats request: {:?}",
        state.name, request
    );

    let content_type_header = request.headers().get(CONTENT_TYPE);
    let content_type = content_type_header.and_then(|value| value.to_str().ok());

    if let Some(content_type) = content_type {
        if content_type.starts_with("application/msgpack") {
            debug!("`{}` server got stats payload.", state.name);

            let body = request.body_mut();
            let compressed_body_bytes = hyper::body::to_bytes(body)
                .await
                .expect("could not decode body into bytes");

            let mut gz = GzDecoder::new(compressed_body_bytes.as_ref());
            let mut decompressed_body_bytes = vec![];
            gz.read_to_end(&mut decompressed_body_bytes)
                .expect("unable to decompress gzip stats payload");

            let payload: StatsPayload = rmp_serde::from_slice(&decompressed_body_bytes).unwrap();

            info!(
                "`{}` server received and deserialized stats payload.",
                state.name
            );
            debug!("{:?}", payload);

            state.tx.send(payload).await.unwrap();
        }
    }
}

#[derive(Serialize)]
struct Span {
    duration: i64,
    error: i32,
    meta: HashMap<String, String>,
    metrics: HashMap<String, f64>,
    name: String,
    parent_id: u64,
    resource: String,
    service: String,
    span_id: i64,
    start: i64,
    trace_id: u64,
    r#type: String,
}

fn build_traces_payload(start: i64, duration: i64, span_id: i64) -> Vec<Vec<Span>> {
    let parent_id = 1;
    let trace_id = 2;
    let resource = "a_resource";
    let service = "a_service";

    let span = Span {
        duration,
        error: 0,
        meta: HashMap::from([("this_is".to_string(), "so_meta".to_string())]),
        metrics: HashMap::from([("_top_level".to_string(), 1.0)]),
        name: "a_name".to_string(),
        parent_id,
        resource: resource.to_string(),
        service: service.to_string(),
        span_id,
        start,
        trace_id,
        r#type: "a_type".to_string(),
    };

    vec![vec![span]]
}

// Sends traces into the Agent container.
// Send two separate requests with thin the same bucket time window to invoke the aggregation logic in the Agent.
async fn send_agent_traces(start: i64, duration: i64, span_id: i64) {
    // sends a trace to each of the urls
    async fn send_trace(urls: &Vec<String>, start: i64, duration: i64, span_id: i64) -> bool {
        let traces_payload = build_traces_payload(start, duration, span_id);
        let client = reqwest::Client::new();

        for url in urls {
            let res = client
                .post(url)
                .header(CONTENT_TYPE, "application/json")
                .json(&traces_payload)
                .send()
                .await
                .unwrap();

            if res.status() != hyper::StatusCode::OK {
                error!("Error sending traces to {}, res: {:?}.", url, res);
                return false;
            }
        }
        info!("Sent a trace to the Agent.");
        true
    }

    // If at a later time we create a more "complete" end to end test which will send data through
    // the full vector topology [agent -> datadog_agent_source -> datadog_traces_sink] , then we
    // can just add the url for the traces endpoint of that container here.
    let urls = vec![trace_agent_only_url()];

    // send first set of trace data
    if !send_trace(&urls, start, duration, span_id + 1).await {
        panic!("can't perform checks if traces aren't accepted by agent.");
    }

    sleep(Duration::from_secs(1)).await;

    // send second set of trace data
    if !send_trace(&urls, start + 1, duration, span_id + 1).await {
        panic!("can't perform checks if traces aren't accepted by agent.");
    }
}

// The sink is run with a max batch size of one, and a stream containing two trace events.
// The two trace events are intentionally configured within the same time bucket window.
// This creates a scenario where the stats payload that is output by the sink after processing the
// *second* batch of events (the second event) >should< contain the aggregated statistics of both
// of the trace events. i.e , the hit count for that bucket should be equal to "2" , not "1".
async fn run_sink_and_send_traces(start: i64, duration: i64, span_id: i64) {
    assert_sink_compliance(&SINK_TAGS, async {
        let config = indoc! {r#"
                default_api_key = "atoken"
                compression = "gzip"
                site = "datadoghq.com"
                endpoint = "http://0.0.0.0:8081"
                batch.max_events = 1
            "#};

        let api_key = std::env::var("TEST_DATADOG_API_KEY")
            .expect("couldn't find the Datatog api key in environment variables");
        assert!(!api_key.is_empty(), "TEST_DATADOG_API_KEY required");

        let config = config.replace("atoken", &api_key);
        let (config, cx) = load_sink::<DatadogTracesConfig>(config.as_str()).unwrap();

        let (sink, _) = config.build(cx).await.unwrap();
        let (batch, receiver) = BatchNotifier::new_with_receiver();

        {
            let traces = vec![
                Event::Trace(
                    simple_trace_event_detailed(
                        "a_resource".to_string(),
                        Some(start),
                        Some(duration),
                        Some(span_id),
                        Some(0),
                    )
                    .with_batch_notifier(&batch),
                ),
                Event::Trace(
                    simple_trace_event_detailed(
                        "a_resource".to_string(),
                        Some(start + 1),
                        Some(duration),
                        Some(span_id + 1),
                        Some(0),
                    )
                    .with_batch_notifier(&batch),
                ),
            ];

            let stream = map_event_batch_stream(stream::iter(traces), Some(batch));

            info!("Sent traces to vector.");
            sink.run(stream).await.unwrap();
        }

        assert_eq!(receiver.await, BatchStatus::Delivered);
    })
    .await;
}

// Receives the stats payloads from the Receiver channels from both of the server instances.
// If either of the servers does not respond with a stats payload, the test will fail.
// The lastest received stats payload is the only one considered. This is the same logic that the
// Datadog UI follows.
// Wait for up to 25 seconds for the stats payload to arrive. The Agent can take some time to send
// the stats out.
// TODO: Looking into if there is a way to configure the agent bucket interval to force the
// flushing to occur faster (reducing the timeout we use and overall runtime of the test)
async fn receive_the_stats(
    rx_agent_only: &mut Receiver<StatsPayload>,
    rx_agent_vector: &mut Receiver<StatsPayload>,
) -> (StatsPayload, StatsPayload) {
    let timeout = sleep(Duration::from_secs(25));
    tokio::pin!(timeout);

    let mut stats_agent_vector = None;
    let mut stats_agent_only = None;

    loop {
        tokio::select! {
            d1 = rx_agent_vector.recv() => {
                stats_agent_vector = d1;
                if stats_agent_only.is_some() && stats_agent_vector.is_some() {
                    break;
                }
            },
            d2 = rx_agent_only.recv() => {
                stats_agent_only = d2;
                if stats_agent_only.is_some() && stats_agent_vector.is_some() {
                    break;
                }
            },
            _ = &mut timeout => break,
        }
    }

    assert!(
        stats_agent_vector.is_some(),
        "received no stats from vector"
    );
    assert!(stats_agent_only.is_some(), "received no stats from agent");

    (stats_agent_only.unwrap(), stats_agent_vector.unwrap())
}

// Compares the stats payload (specifically the bucket for the time window we sent events on)
// between the Vector and Agent for consistency.
fn validate_stats(agent_stats: &StatsPayload, vector_stats: &StatsPayload) {
    let agent_bucket = agent_stats.stats.first().unwrap().stats.first().unwrap();

    let vector_bucket = vector_stats.stats.first().unwrap().stats.first().unwrap();
    assert!(
        agent_bucket.start == vector_bucket.start,
        "bucket start times do not match"
    );
    assert!(
        agent_bucket.duration == vector_bucket.duration,
        "bucket durations do not match"
    );

    let agent_s = agent_bucket.stats.first().unwrap();
    let vector_s = vector_bucket.stats.first().unwrap();

    info!("\nagent_stats : {:?}", agent_s);
    info!("\nvector_stats : {:?}", vector_s);

    assert!(agent_s.service == vector_s.service);
    assert!(agent_s.name == vector_s.name);
    assert!(agent_s.resource == vector_s.resource);
    assert!(agent_s.http_status_code == vector_s.http_status_code);
    assert!(agent_s.r#type == vector_s.r#type);
    assert!(agent_s.db_type == vector_s.db_type);
    assert!(agent_s.hits == vector_s.hits);
    assert!(agent_s.errors == vector_s.errors);
    assert!(agent_s.duration == vector_s.duration);
    assert!(agent_s.synthetics == vector_s.synthetics);
    assert!(agent_s.top_level_hits == vector_s.top_level_hits);
}

#[tokio::test]
async fn apm_stats_e2e_test_dd_agent_to_vector_correctness() {
    trace_init();

    // channels for the servers to send us back data on
    let (tx_agent_vector, mut rx_agent_vector) = mpsc::channel(32);
    let (tx_agent_only, mut rx_agent_only) = mpsc::channel(32);

    // spawn the servers
    {
        // [vector -> the server]
        tokio::spawn(async move {
            run_server("vector".to_string(), vector_port(), tx_agent_vector).await;
        });

        // [agent -> the server]
        tokio::spawn(async move {
            run_server("agent".to_string(), agent_port(), tx_agent_only).await;
        });
    }

    // allow the agent to start up
    sleep(Duration::from_secs(5)).await;

    let start = Utc::now().timestamp_nanos();
    let duration = 20;
    let span_id = 3;

    // starts the sink and sends the traces through it
    // panics if the batch status was not Delivered.
    run_sink_and_send_traces(start, duration, span_id).await;

    // sends the traces through the agent
    // panics if the HTTP post fails
    send_agent_traces(start, duration, span_id).await;

    // receive the stats on the channel receivers from the servers
    let (stats_agent, stats_vector) =
        receive_the_stats(&mut rx_agent_only, &mut rx_agent_vector).await;

    // compare the stats from agent and vector for consistency
    validate_stats(&stats_agent, &stats_vector);
}

#[tokio::test]
async fn to_real_traces_endpoint() {
    assert_sink_compliance(&SINK_TAGS, async {
        let config = indoc! {r#"
            default_api_key = "atoken"
            compression = "none"
        "#};
        let api_key = std::env::var("TEST_DATADOG_API_KEY")
            .expect("couldn't find the Datatog api key in environment variables");
        assert!(!api_key.is_empty(), "TEST_DATADOG_API_KEY required");
        let config = config.replace("atoken", &api_key);
        let (config, cx) = load_sink::<DatadogTracesConfig>(config.as_str()).unwrap();

        let (sink, _) = config.build(cx).await.unwrap();
        let (batch, receiver) = BatchNotifier::new_with_receiver();

        let trace = vec![Event::Trace(
            simple_trace_event("a_trace".to_string()).with_batch_notifier(&batch),
        )];

        let stream = map_event_batch_stream(stream::iter(trace), Some(batch));

        sink.run(stream).await.unwrap();
        assert_eq!(receiver.await, BatchStatus::Delivered);
    })
    .await;
}
