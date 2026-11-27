use std::time::Instant;

use vector_lib::{
    NamedInternalEvent, gauge,
    internal_event::{GaugeName, InternalEvent, LossyF64},
};

use crate::built_info;

#[derive(Debug, NamedInternalEvent)]
pub struct Heartbeat {
    pub since: Instant,
}

impl InternalEvent for Heartbeat {
    fn emit(self) {
        // Periodic by design (1 Hz); a 1-minute window avoids rate-limiter chatter.
        trace!(target: "vector", message = "Beep.", internal_log_rate_secs = 60);
        gauge!(GaugeName::UptimeSeconds).set(LossyF64(self.since.elapsed().as_secs()));
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
