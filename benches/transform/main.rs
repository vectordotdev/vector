use criterion::criterion_main;

mod common;
mod filter;

criterion_main!(filter::benches);
