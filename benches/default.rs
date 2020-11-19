use criterion::criterion_main;

mod batch;
mod buffering;
mod event;
mod files;
mod http;
mod isolated_buffering;
mod lua;
mod regex;
mod remap;
mod template;
mod topology;

criterion_main!(
    batch::benches,
    buffering::benches,
    event::benches,
    files::benches,
    http::benches,
    isolated_buffering::benches,
    lua::benches,
    regex::benches,
    remap::benches,
    template::benches,
    topology::benches,
);
