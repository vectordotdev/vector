use criterion::criterion_main;

mod common;
mod dedupe;
mod filter;
mod reduce;

criterion_main!(reduce::benches, dedupe::benches, filter::benches,);
