use vector_lib::lookup::lookup_v2::{OptionalTargetPath, OptionalValuePath};

pub mod logs;
pub mod metrics;

pub fn config_host_key_target_path() -> OptionalTargetPath {
    OptionalTargetPath {
        path: crate::config::log_schema().host_key_target_path().cloned(),
    }
}

pub fn config_host_key() -> OptionalValuePath {
    OptionalValuePath {
        path: crate::config::log_schema().host_key().cloned(),
    }
}
