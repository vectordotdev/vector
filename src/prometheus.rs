pub const METRIC_NAME_LABEL: &str = "__name__";

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/prometheus.rs"));

    pub use metric_metadata::MetricType;

    impl MetricType {
        pub fn as_str(&self) -> &'static str {
            match self {
                MetricType::Counter => "counter",
                MetricType::Gauge => "gauge",
                MetricType::Histogram => "histogram",
                MetricType::Summary => "summary",
                MetricType::Gaugehistogram => "gaugehistogram",
                MetricType::Info => "info",
                MetricType::Stateset => "stateset",
                MetricType::Unknown => "unknown",
            }
        }
    }
}
