use criterion::criterion_main;

mod common;
mod dedupe;
mod filter;
mod reduce;

criterion_main!(dedupe::benches, filter::benches, reduce::benches,);
