use criterion::criterion_main;

mod character_delimited_bytes;
mod encoder;
mod newline_bytes;
mod octet_counting_framing;
mod otlp;

criterion_main!(
    character_delimited_bytes::benches,
    newline_bytes::benches,
    encoder::benches,
    otlp::benches,
    octet_counting_framing::benches,
);
