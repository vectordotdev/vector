use criterion::criterion_main;

mod source_sender;

criterion_main!(source_sender::benches);
