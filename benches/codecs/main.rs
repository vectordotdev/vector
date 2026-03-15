use criterion::criterion_main;

mod character_delimited_bytes;
mod encoder;
mod newline_bytes;
#[cfg(feature = "codecs-parquet")]
mod parquet;

#[cfg(not(feature = "codecs-parquet"))]
criterion_main!(
    character_delimited_bytes::benches,
    newline_bytes::benches,
    encoder::benches,
);

#[cfg(feature = "codecs-parquet")]
criterion_main!(
    character_delimited_bytes::benches,
    newline_bytes::benches,
    encoder::benches,
    parquet::benches,
);
