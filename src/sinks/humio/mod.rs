pub mod logs;
pub mod metrics;

fn default_host_key() -> String {
    crate::config::LogSchema::default().host_key().to_string()
}
