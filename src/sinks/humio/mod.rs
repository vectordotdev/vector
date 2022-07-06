pub mod logs;
pub mod metrics;

fn host_key() -> String {
    crate::config::log_schema().host_key().to_string()
}
