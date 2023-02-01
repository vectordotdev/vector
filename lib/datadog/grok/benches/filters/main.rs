use criterion::criterion_main;

mod keyvalue;

criterion_main!(keyvalue::benches);
