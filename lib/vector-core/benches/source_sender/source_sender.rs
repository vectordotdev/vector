use std::time::Duration;

use criterion::{BenchmarkGroup, Criterion, SamplingMode, criterion_group, measurement::WallTime};
use lookup::event_path;
use tokio::runtime::Runtime;
use vector_core::{
    config::{ComponentKey, DataType, SourceOutput},
    event::{Event, LogEvent},
    schema::Definition,
    source_sender::SourceSender,
};

fn bench_config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_secs(5))
        .measurement_time(Duration::from_secs(60))
        // degree of noise to ignore in measurements, here 1%
        .noise_threshold(0.01)
        // likelihood of noise registering as difference, here 5%
        .significance_level(0.05)
        // likelihood of capturing the true runtime, here 95%
        .confidence_level(0.95)
        // total number of bootstrap resamples, higher is less noisy but slower
        .nresamples(100_000)
        // total samples to collect within the set measurement time
        .sample_size(150)
}

/// Builds a `SourceSender` with one default log output, mirroring how log
/// sources construct their sender, and drains the receiving channel in the
/// background so sends never block.
fn sender_with_drain(rt: &Runtime) -> SourceSender {
    let output = SourceOutput::new_maybe_logs(DataType::Log, Definition::any());
    let mut builder = SourceSender::builder().with_buffer(100);
    let rx = builder.add_source_output(output, ComponentKey::from("bench_source"));
    let sender = builder.build();

    rt.spawn(async move {
        // Hold the receiver for the lifetime of the benchmark so sends succeed;
        // continuously drain to keep the channel from applying backpressure.
        let mut rx = rx;
        while rx.next().await.is_some() {}
    });
    sender
}

fn default_event() -> Event {
    let mut log = LogEvent::default();
    log.insert(
        event_path!("message"),
        "127.0.0.1 - - [10/Oct/2000:13:55:36 -0700] \"GET /apache_pb.gif HTTP/1.0\" 200 2326",
    );
    log.into()
}

/// Measures the per-request clone that HTTP sources perform when handing the
/// sender to the request handler.
fn clone_sender(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let sender = sender_with_drain(&rt);

    let mut group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector_core::source_sender::SourceSender::clone");
    group.sampling_mode(SamplingMode::Auto);
    group.bench_function("clone (default output)", |b| b.iter(|| sender.clone()));
    group.finish();
}

/// Measures a full request-shaped cycle: clone the sender, send a single
/// event batch, then drop the clone. This is the `http_server` source path.
fn clone_and_send(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let sender = sender_with_drain(&rt);

    let mut group: BenchmarkGroup<WallTime> =
        c.benchmark_group("vector_core::source_sender::SourceSender::request_cycle");
    group.sampling_mode(SamplingMode::Auto);
    group.bench_function("clone + send_event (1 event)", |b| {
        b.iter(|| {
            let mut out = sender.clone();
            let _guard = rt.enter();
            rt.block_on(async {
                // Deliberately ignore the result: a closed channel is not a
                // benchmark failure signal.
                _ = out.send_event(default_event()).await;
            });
        })
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = bench_config();
    targets = clone_sender, clone_and_send
}
