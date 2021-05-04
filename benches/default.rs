use criterion::criterion_main;

mod batch;
mod buffer;
mod event;
mod files;
mod http;
mod isolated_buffer;
mod lua;
mod metrics_snapshot;
mod regex;
mod template;
mod topology;

criterion_main!(
    batch::benches,
    buffer::benches,
    event::benches,
    files::benches,
    http::benches,
    isolated_buffer::benches,
    lua::benches,
    metrics_snapshot::benches,
    regex::benches,
    template::benches,
    topology::benches,
);
