use axum::{
    body::Body,
    extract::Extension,
    http::{header::CONTENT_TYPE, Request},
    routing::{get, post},
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

fn agent_to_vector_port() -> u16 {
    std::env::var("AGENT_TO_VECTOR_PORT")
        .unwrap_or_else(|_| "8081".to_string())
        .parse::<u16>()
        .unwrap()
}

fn agent_only_port() -> u16 {
    std::env::var("AGENT_ONLY_PORT")
        .unwrap_or_else(|_| "8082".to_string())
        .parse::<u16>()
        .unwrap()
}

fn trace_agent_only_url() -> String {
    std::env::var("TRACE_AGENT_ONLY_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8126/v0.3/traces".to_owned())
}

//fn trace_agent_vector_url() -> String {
//    std::env::var("TRACE_AGENT_VECTOR_URL")
//        .unwrap_or_else(|_| "http://127.0.0.1:8126/v0.3/traces".to_owned())
//}

// Shared state for the HTTP server
struct AppState {
    tx: Sender<StatsPayload>,
}

// build the app with a route and run our app with hyper
async fn run_server(port: u16, tx: Sender<StatsPayload>) {
    let state = Arc::new(AppState { tx });
    let app = Router::new()
        .route("/", get(root))
        .route("/api/v0.2/traces", post(process_traces))
        .route("/api/v0.2/stats", post(process_stats))
        .layer(Extension(state));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    //println!("{:?}", addr);

    println!("listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// basic handler that responds with a static string
async fn root(Extension(_state): Extension<Arc<AppState>>) -> &'static str {
    ""
}

/// TODO
async fn process_traces(Extension(_state): Extension<Arc<AppState>>, request: Request<Body>) {
    let content_type_header = request.headers().get(CONTENT_TYPE);
    let content_type = content_type_header.and_then(|value| value.to_str().ok());

    if let Some(content_type) = content_type {
        if content_type.starts_with("application/x-protobuf") {
            println!("got trace payload!");
        }
    }
}

/// TODO
async fn process_stats(Extension(state): Extension<Arc<AppState>>, mut request: Request<Body>) {
    println!("process_stats request: {:?}", request);

    let content_type_header = request.headers().get(CONTENT_TYPE);
    let content_type = content_type_header.and_then(|value| value.to_str().ok());

    if let Some(content_type) = content_type {
        if content_type.starts_with("application/msgpack") {
            println!("got stats payload!");

            let body = request.body_mut();
            let compressed_body_bytes = hyper::body::to_bytes(body)
                .await
                .expect("could not decode body into bytes");

            let mut gz = GzDecoder::new(compressed_body_bytes.as_ref());
            let mut decompressed_body_bytes = vec![];
            gz.read_to_end(&mut decompressed_body_bytes)
                .expect("unable to decompress gzip stats payload");

            let payload: StatsPayload = rmp_serde::from_slice(&decompressed_body_bytes).unwrap();

            println!("deserialized stats payload: {:?}", payload);

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

async fn feed_traces(urls: &Vec<String>, start: i64, duration: i64, span_id: i64) -> bool {
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
            println!("error feeding traces to {}, res: {:?}", url, res);
            return false;
        }
    }
    true
}

async fn run_sink(start: i64, duration: i64, span_id: i64) {
    assert_sink_compliance(&SINK_TAGS, async {
        let config = indoc! {r#"
                default_api_key = "atoken"
                compression = "gzip"
                site = "datadoghq.com"
                endpoint = "http://0.0.0.0:8081"
            "#};

        let api_key = std::env::var("TEST_DATADOG_API_KEY")
            .expect("couldn't find the Datatog api key in environment variables");
        assert!(!api_key.is_empty(), "TEST_DATADOG_API_KEY required");

        let config = config.replace("atoken", &api_key);
        let (config, cx) = load_sink::<DatadogTracesConfig>(config.as_str()).unwrap();

        let (sink, _) = config.build(cx).await.unwrap();
        let (batch, receiver) = BatchNotifier::new_with_receiver();

        {
            let trace = vec![Event::Trace(
                simple_trace_event_detailed(
                    "a_resource".to_string(),
                    Some(start),
                    Some(duration),
                    Some(span_id),
                )
                .with_batch_notifier(&batch),
            )];

            let stream = map_event_batch_stream(stream::iter(trace), Some(batch));

            sink.run(stream).await.unwrap();
        }

        // TODO how to send events in separate batches

        //sleep(Duration::from_secs(1)).await;

        //{
        //    let trace = vec![Event::Trace(
        //        simple_trace_event_detailed(
        //            "a_resource".to_string(),
        //            Some(start + 1),
        //            Some(duration),
        //            Some(span_id + 1),
        //        )
        //        .with_batch_notifier(&batch),
        //    )];

        //    let stream = map_event_batch_stream(stream::iter(trace), Some(batch));

        //    sink.run(stream).await.unwrap();
        //}

        assert_eq!(receiver.await, BatchStatus::Delivered);
    })
    .await;
}

async fn receive_the_stats(
    rx_agent_only: &mut Receiver<StatsPayload>,
    rx_agent_vector: &mut Receiver<StatsPayload>,
) -> (StatsPayload, StatsPayload) {
    let timeout = sleep(Duration::from_secs(30));
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

    println!();
    println!("agent_stats : {:?}", agent_s);
    println!();
    println!("vector_stats : {:?}", vector_s);
    println!();

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
        // [agent -> vector -> the server]
        tokio::spawn(async move {
            run_server(agent_to_vector_port(), tx_agent_vector).await;
        });

        // [agent -> the server]
        tokio::spawn(async move {
            run_server(agent_only_port(), tx_agent_only).await;
        });
    }

    // allow the agent to start up
    sleep(Duration::from_secs(10)).await;

    let start = Utc::now().timestamp_nanos();
    let duration = 20;
    let span_id = 3;

    // start the sink
    run_sink(start, duration, span_id).await;

    // feed in the traces
    {
        //let urls = vec![trace_agent_only_url(), trace_agent_vector_url()];
        let urls = vec![trace_agent_only_url()];

        // feed first set of trace data
        if !feed_traces(&urls, start, duration, span_id + 1).await {
            panic!("can't perform checks if traces aren't accepted");
        }

        sleep(Duration::from_secs(1)).await;

        // feed second set of trace data
        if !feed_traces(&urls, start + 1, duration, span_id + 1).await {
            panic!("can't perform checks if traces aren't accepted");
        }
    }

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
