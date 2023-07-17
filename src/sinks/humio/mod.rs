use lookup::lookup_v2::OptionalValuePath;

pub mod logs;
pub mod metrics;

pub fn host_key() -> OptionalValuePath {
    OptionalValuePath {
        path: crate::config::log_schema().host_key().cloned(),
    }
}
