use std::time::Instant;

use vector_lib::{
    NamedInternalEvent, gauge,
    internal_event::{GaugeName, InternalEvent, ToF64Lossy},
};

use crate::built_info;

#[derive(Debug, NamedInternalEvent)]
pub struct Heartbeat {
    pub since: Instant,
}

impl InternalEvent for Heartbeat {
    fn emit(self) {
        trace!(target: "vector", message = "Beep.");
        gauge!(GaugeName::UptimeSeconds).set(self.since.elapsed().as_secs().to_f64_lossy());
        gauge!(
            GaugeName::BuildInfo,
            "debug" => built_info::DEBUG,
            "version" => built_info::PKG_VERSION,
            "rust_version" => built_info::RUST_VERSION,
            "arch" => built_info::TARGET_ARCH,
            "revision" => built_info::VECTOR_BUILD_DESC.unwrap_or("")
        )
        .set(1.0);
    }
}
