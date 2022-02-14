#[cfg(all(
    test,
    feature = "sinks-blackhole",
    feature = "sources-stdin",
    feature = "transforms-json_parser"
))]
mod transient_state;

#[cfg(all(test, feature = "sinks-console", feature = "sources-demo_logs"))]
mod source_finished;

#[cfg(all(
    test,
    feature = "sinks-console",
    feature = "sources-splunk_hec",
    feature = "sources-demo_logs",
    feature = "sinks-prometheus",
    feature = "transforms-log_to_metric",
    feature = "sinks-socket",
    feature = "leveldb"
))]
mod reload;

#[cfg(all(test, feature = "sinks-console", feature = "sources-socket"))]
mod doesnt_reload;

#[cfg(test)]
mod backpressure;
