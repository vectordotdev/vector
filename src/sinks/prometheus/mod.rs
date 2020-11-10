mod collector;
pub mod exporter;
mod remote_write;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/prometheus.rs"));
}

pub(self) fn default_histogram_buckets() -> Vec<f64> {
    vec![
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ]
}

pub(self) fn default_summary_quantiles() -> Vec<f64> {
    vec![0.5, 0.75, 0.9, 0.95, 0.99]
}
