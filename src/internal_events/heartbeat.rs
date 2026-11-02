use std::time::Instant;

use vector_lib::{
    NamedInternalEvent, gauge,
    internal_event::{GaugeName, InternalEvent},
};

use crate::built_info;

#[derive(Debug, NamedInternalEvent)]
pub struct Heartbeat {
    pub since: Instant,
}

impl InternalEvent for Heartbeat {
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(clippy::cast_precision_loss, reason = "Metrics use f64")]
    fn emit(self) {
        trace!(target: "vector", message = "Beep.");
        gauge!(GaugeName::UptimeSeconds).set(self.since.elapsed().as_secs() as f64);
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
