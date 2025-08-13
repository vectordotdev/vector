use crate::internal_events::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ConfigReloadRejected {
    reason: ReloadRejectReason,
}

impl InternalEvent for ConfigReloadRejected {
    fn emit(self) {
        match self.reason {
            ReloadRejectReason::GlobalOptionsChanged(items) => {
                error!(
                    message = "Config reload rejected due to non-reloadable global options.",
                    reason = "global_options_changed",
                    changed_fields = %fields.join(", "),
                    internal_log_rate_limit = true,
                );

                gauge!(
                    "vector_config_reload_rejected",
                    "reason" => "global_options_changed",
                )
                .set(1.0);
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
}

enum ReloadRejectReason {
    GlobalOptionsChanged { fields: Vec<String> },
}

impl ReloadRejectReason {
    fn as_str(&self) -> &'static str {
        match self {
            Self::GlobalOptionsChanged => "global_options changed",
        }
    }
}
