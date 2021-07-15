use criterion::criterion_main;

mod common;
mod log_event;

criterion_main!(log_event::benches);
