use criterion::criterion_main;

mod wasm_benchmarks;

criterion_main! {
    wasm_benchmarks::benches,
}
