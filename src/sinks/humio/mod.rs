pub mod logs;
pub mod metrics;

fn host_key() -> String {
    crate::config::log_schema().host_key().unwrap_or_default().to_string()
}
