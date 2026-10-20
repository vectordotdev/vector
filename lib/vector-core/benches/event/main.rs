use criterion::criterion_main;

mod log_event;
mod metric_tags;

criterion_main!(log_event::benches, metric_tags::benches);
