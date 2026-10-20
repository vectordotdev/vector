use criterion::criterion_main;

mod character_delimited_bytes;
mod encoder;
mod newline_bytes;
mod otlp;

criterion_main!(
    character_delimited_bytes::benches,
    newline_bytes::benches,
    encoder::benches,
    otlp::benches,
);
