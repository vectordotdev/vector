use criterion::criterion_main;

mod default_benchmarks;

criterion_main!(
    default_benchmarks::batch::benches,
    default_benchmarks::buffering::benches,
    default_benchmarks::event::benches,
    default_benchmarks::files::benches,
    default_benchmarks::http::benches,
    default_benchmarks::isolated_buffering::benches,
    // Pending https://github.com/timberio/vector/pull/4875 to be able to clone function transforms
    //default_benchmarks::lua::benches,
    default_benchmarks::regex::benches,
    default_benchmarks::remap::benches,
    default_benchmarks::template::benches,
    default_benchmarks::topology::benches,
);
