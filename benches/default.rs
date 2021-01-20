use criterion::criterion_main;

mod batch;
mod buffering;
mod event;
mod files;
mod http;
mod isolated_buffering;
mod lookup;
mod lua;
mod metrics_snapshot;
mod regex;
mod template;
mod topology;

criterion_main!(
    batch::benches,
    buffering::benches,
    event::benches,
    files::benches,
    http::benches,
    isolated_buffering::benches,
    lookup::benches,
    lua::benches,
    metrics_snapshot::benches,
    regex::benches,
    template::benches,
    topology::benches,
);
