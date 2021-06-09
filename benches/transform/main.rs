use criterion::criterion_main;

mod common;
mod dedupe;
mod filter;

criterion_main!(dedupe::benches, filter::benches,);
