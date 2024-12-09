use std::time::Instant;

use crate::built_info;
use metrics::gauge;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct Heartbeat {
    pub since: Instant,
}

impl InternalEvent for Heartbeat {
    fn emit(self) {
        trace!(target: "vector", message = "Beep.");
        gauge!("uptime_seconds").set(self.since.elapsed().as_secs() as f64);
        gauge!(
            "build_info",
            "debug" => built_info::DEBUG,
            "version" => built_info::PKG_VERSION,
            "rust_version" => built_info::RUST_VERSION,
            "arch" => built_info::TARGET_ARCH,
            "revision" => built_info::VECTOR_BUILD_DESC.unwrap_or("")
        )
        .set(1.0);
    }
}
