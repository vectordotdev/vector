use metrics::counter;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct ConfigReloadRejected {
    reason: ReloadRejectReason,
}

impl InternalEvent for ConfigReloadRejected {
    fn emit(self) {
        match &self.reason {
            ReloadRejectReason::GlobalOptionsChanged { fields } => {
                error!(
                    message = "Config reload rejected due to non-reloadable global options.",
                    reason = %self.reason.as_str(),
                    changed_fields = %fields.join(", "),
                    internal_log_rate_limit = false,
                );

                counter!(
                    "config_reload_rejected",
                    "reason" => self.reason.as_str(),
                )
                .increment(1);
            }
            ReloadRejectReason::FailedToComputeGlobalDiff(err) => {
                error!(
                    message = "Config reload rejected due to failed to compute global diff.",
                    reason = %self.reason.as_str(),
                    error = %err,
                    internal_log_rate_limit = false,
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
    pub const fn global_options_changed(fields: Vec<String>) -> Self {
        Self {
            reason: ReloadRejectReason::GlobalOptionsChanged { fields },
        }
    }

    pub const fn failed_to_compute_global_diff(error: serde_json::Error) -> Self {
        Self {
            reason: ReloadRejectReason::FailedToComputeGlobalDiff(error),
        }
    }
}

#[derive(Debug)]
enum ReloadRejectReason {
    GlobalOptionsChanged { fields: Vec<String> },
    FailedToComputeGlobalDiff(serde_json::Error),
}

impl ReloadRejectReason {
    const fn as_str(&self) -> &'static str {
        match self {
            Self::GlobalOptionsChanged { fields: _ } => "global_options changed",
            Self::FailedToComputeGlobalDiff(_) => "failed to compute global diff",
        }
    }
}

#[derive(Debug)]
pub struct ConfigReloaded;

impl InternalEvent for ConfigReloaded {
    fn emit(self) {
        info!("New configuration loaded successfully.");

        counter!("config_reloaded",).increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("ConfigReloaded")
    }
}
