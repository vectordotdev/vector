use metrics::{counter, gauge};
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct ConfigReloadRejected {
    reason: ReloadRejectReason,
}

impl InternalEvent for ConfigReloadRejected {
    fn emit(self) {
        match self.reason {
            ReloadRejectReason::GlobalOptionsChanged { ref fields } => {
                error!(
                    message = "Config reload rejected due to non-reloadable global options.",
                    reason = %self.reason.as_str(),
                    changed_fields = %fields.join(", "),
                    internal_log_rate_limit = true,
                );

                counter!(
                    "config_reload_rejected",
                    "reason" => self.reason.as_str(),
                )
                .increment(1);
            }
            ReloadRejectReason::FailedToComputeGlobalDiff => {
                error!(
                    message = "Config reload rejected due to failed to compute global diff.",
                    reason = %self.reason.as_str(),
                    internal_log_rate_limit = true,
                );

                counter!(
                    "config_reload_rejected",
                    "reason" => self.reason.as_str(),
                )
                .increment(1);
            }
        }
    }

    fn name(&self) -> Option<&'static str> {
        Some("ConfigReloadRejected")
    }
}

impl ConfigReloadRejected {
    pub fn global_options_changed(fields: Vec<String>) -> Self {
        Self {
            reason: ReloadRejectReason::GlobalOptionsChanged { fields },
        }
    }

    pub fn failed_to_compute_global_diff() -> Self {
        Self {
            reason: ReloadRejectReason::FailedToComputeGlobalDiff,
        }
    }
}

#[derive(Debug)]
enum ReloadRejectReason {
    GlobalOptionsChanged { fields: Vec<String> },
    FailedToComputeGlobalDiff,
}

impl ReloadRejectReason {
    fn as_str(&self) -> &'static str {
        match self {
            Self::GlobalOptionsChanged { fields: _ } => "global_options changed",
            Self::FailedToComputeGlobalDiff => "failed to compute global diff",
        }
    }
}

#[derive(Debug)]
pub struct ConfigReloaded {}

impl InternalEvent for ConfigReloaded {
    fn emit(self) {
        info!(
            message = "New configuration loaded successfully.",
            internal_log_rate_limit = true,
        );

        counter!("config_reloaded",).increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("ConfigReloaded")
    }
}
