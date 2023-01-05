use std::path::PathBuf;

use vector_config::configurable_component;

use super::{FilterList, HostMetrics, MetricsBuffer};

/// Options for the “cgroups” (controller groups) metrics collector.
///
/// This collector is only usable on Linux systems, and only supports either version 2 or hybrid cgroups.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(default)]
pub(crate) struct CGroupsConfig {
    /// The number of levels of the cgroups hierarchy for which to report metrics.
    ///
    /// A value of `1` means just the root or named cgroup.
    #[derivative(Default(value = "100"))]
    levels: usize,

    /// The base cgroup name to provide metrics for.
    pub(super) base: Option<PathBuf>,

    /// Lists of group name patterns to include or exclude.
    groups: FilterList,

    /// Base cgroup directory, for testing use only
    #[serde(skip_serializing)]
    base_dir: Option<PathBuf>,
}

impl HostMetrics {
    #[cfg(not(target_os = "linux"))]
    pub(super) async fn cgroups_metrics(&self, _output: &mut MetricsBuffer) {
        todo!("warning")
    }

    #[cfg(target_os = "linux")]
    pub(super) async fn cgroups_metrics(&self, output: &mut MetricsBuffer) {
        if let Some(config) = &self.config.cgroups {
            if let Some(root) = &self.root_cgroup {
                output.name = "cgroups";
                let mut recurser = linux::CGroupRecurser::new(config, output);
                match &root.mode {
                    Mode::Modern(base) => recurser.scan_modern(root, base).await,
                    Mode::Legacy(base) => recurser.scan_legacy(root, base).await,
                    Mode::Hybrid(v1base, v2base) => {
                        // Hybrid cgroups contain both legacy and modern cgroups, so scan them both
                        // for the data files. The `cpu` controller is usually found in the modern
                        // groups, but the top-level stats are found under the legacy controller in
                        // some setups. Similarly, the `memory` controller can be found in either
                        // location. As such, detecting exactly where to scan for the controllers
                        // doesn't work, so opportunistically scan for any controller files in all
                        // subdirectories of the given root.
                        recurser.scan_legacy(root, v1base).await;
                        recurser.scan_modern(root, v2base).await;
                    }
                }
            }
        }
    }
}

//#[cfg(not(target_os = "linux"))]
//#[path = "cgroups_nonlinux.rs"]
//mod cgroups;
//#[cfg(target_os = "linux")]
//#[path = "cgroups_linux.rs"]
