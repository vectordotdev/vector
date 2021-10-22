use criterion::criterion_main;

mod log_event;

criterion_main!(log_event::benches);
