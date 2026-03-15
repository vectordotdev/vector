use criterion::criterion_main;

mod common;
mod dedupe;
mod filter;
mod reduce;
mod route;
mod throttle;

criterion_main!(
    dedupe::benches,
    filter::benches,
    reduce::benches,
    route::benches,
    throttle::benches,
);
