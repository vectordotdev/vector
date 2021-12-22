use criterion::criterion_main;

mod character_delimited_bytes;
mod newline_bytes;

criterion_main!(character_delimited_bytes::benches, newline_bytes::benches,);
