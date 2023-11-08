use vector_lib::lookup::lookup_v2::OptionalTargetPath;

pub mod logs;
pub mod metrics;

pub fn config_host_key() -> OptionalTargetPath {
    OptionalTargetPath {
        path: crate::config::log_schema().host_key_target_path().cloned(),
    }
}
