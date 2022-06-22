#[cfg(all(
    feature = "sinks-blackhole",
    feature = "sources-stdin",
    feature = "transforms-json_parser"
))]
mod transient_state;

#[cfg(all(feature = "sinks-console", feature = "sources-demo_logs"))]
mod source_finished;

#[cfg(all(
    feature = "sources-prometheus",
    feature = "sinks-prometheus",
    feature = "sources-internal_metrics",
    feature = "sinks-blackhole",
))]
mod reload;

#[cfg(all(feature = "sinks-console", feature = "sources-socket"))]
mod doesnt_reload;

mod backpressure;
mod compliance;
