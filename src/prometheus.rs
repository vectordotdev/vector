pub const METRIC_NAME_LABEL: &str = "__name__";

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/prometheus.rs"));
}
