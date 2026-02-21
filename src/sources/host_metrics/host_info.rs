use sysinfo::System;
use vector_lib::event::MetricTags;

use super::HostMetrics;

const HOST_INFO: &str = "host_info";

impl HostMetrics {
    pub fn host_info_metrics(&self, output: &mut super::MetricsBuffer) {
        output.name = "hostinfo";

        let mut tags = MetricTags::default();

        // OS info (sysinfo)
        if let Some(name) = System::name() {
            tags.replace("os_name".into(), name);
        }
        if let Some(version) = System::os_version() {
            tags.replace("os_version".into(), version);
        }
        if let Some(kernel) = System::kernel_version() {
            tags.replace("kernel_version".into(), kernel);
        }
        if let Some(hostname) = System::host_name() {
            tags.replace("hostname".into(), hostname);
        }

        // CPU info (sysinfo)
        {
            let mut sys = System::new();
            sys.refresh_cpu_all();
            if let Some(cpu) = sys.cpus().first() {
                let brand = cpu.brand().trim().to_string();
                if !brand.is_empty() {
                    tags.replace("cpu_model".into(), brand);
                }
                let vendor = cpu.vendor_id().trim().to_string();
                if !vendor.is_empty() {
                    tags.replace("cpu_vendor".into(), vendor);
                }
            }
        }

        // Architecture
        tags.replace("arch".into(), std::env::consts::ARCH.to_string());

        // Network info (netdev) — primary non-loopback interface
        if let Ok(default_iface) = netdev::get_default_interface() {
            if let Some(ipv4) = default_iface.ipv4.first() {
                tags.replace("ip".into(), ipv4.addr().to_string());
            }
            if let Some(mac) = default_iface.mac_addr {
                tags.replace("mac_address".into(), mac.to_string());
            }
        }

        // VM UUID
        if let Some(uuid) = read_vm_uuid() {
            tags.replace("vm_uuid".into(), uuid);
        }

        // Boot ID (Linux only)
        #[cfg(target_os = "linux")]
        if let Some(boot_id) = read_file_trimmed("/proc/sys/kernel/random/boot_id") {
            tags.replace("boot_id".into(), boot_id);
        }

        // Cloud provider detection
        if let Some(provider) = detect_cloud_provider() {
            tags.replace("cloud_provider".into(), provider);
        }

        // Virtualization type
        if let Some(virt_type) = detect_virtualization_type() {
            tags.replace("virtualization_type".into(), virt_type);
        }

        // Container detection
        tags.replace("is_container".into(), detect_is_container().to_string());

        // Timezone
        if let Some(tz) = detect_timezone() {
            tags.replace("timezone".into(), tz);
        }

        // Locale
        if let Ok(locale) = std::env::var("LANG") {
            if !locale.is_empty() {
                tags.replace("locale".into(), locale);
            }
        }

        // Domain
        if let Some(domain) = read_domain() {
            tags.replace("domain".into(), domain);
        }

        // Vector version
        tags.replace(
            "vector_version".into(),
            crate::built_info::PKG_VERSION.to_string(),
        );

        output.gauge(HOST_INFO, 1.0, tags);
    }
}

#[cfg(target_os = "linux")]
fn read_file_trimmed(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_vm_uuid() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        // Try DMI product_uuid (requires root or relaxed permissions)
        if let Some(uuid) = read_file_trimmed("/sys/devices/virtual/dmi/id/product_uuid") {
            return Some(uuid);
        }
        read_file_trimmed("/sys/class/dmi/id/product_uuid")
    }
    #[cfg(target_os = "windows")]
    {
        // Read MachineGuid from Windows registry
        read_windows_registry("SOFTWARE\\Microsoft\\Cryptography", "MachineGuid")
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

fn detect_cloud_provider() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        // Check DMI board_vendor / sys_vendor / chassis_asset_tag for cloud hints
        let checks: &[&str] = &[
            "/sys/class/dmi/id/board_vendor",
            "/sys/class/dmi/id/sys_vendor",
            "/sys/class/dmi/id/chassis_asset_tag",
            "/sys/class/dmi/id/product_name",
        ];
        for path in checks {
            if let Some(val) = read_file_trimmed(path) {
                let lower = val.to_lowercase();
                if lower.contains("microsoft") || lower.contains("azure") {
                    return Some("azure".into());
                }
                if lower.contains("amazon") || lower.contains("aws") || lower.contains("ec2") {
                    return Some("aws".into());
                }
                if lower.contains("google") || lower.contains("gcp") {
                    return Some("gcp".into());
                }
            }
        }
        None
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(val) = read_windows_registry(
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\OEMInformation",
            "Manufacturer",
        ) {
            let lower = val.to_lowercase();
            if lower.contains("microsoft") || lower.contains("azure") {
                return Some("azure".into());
            }
            if lower.contains("amazon") || lower.contains("aws") {
                return Some("aws".into());
            }
            if lower.contains("google") || lower.contains("gcp") {
                return Some("gcp".into());
            }
        }
        None
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

fn detect_virtualization_type() -> Option<String> {
    // Container detection first
    if std::path::Path::new("/.dockerenv").exists() {
        return Some("docker".into());
    }
    if std::path::Path::new("/run/.containerenv").exists() {
        return Some("podman".into());
    }

    #[cfg(target_os = "linux")]
    {
        // Check cgroup for container hints
        if let Some(cgroup) = read_file_trimmed("/proc/1/cgroup") {
            if cgroup.contains("docker") {
                return Some("docker".into());
            }
            if cgroup.contains("lxc") {
                return Some("lxc".into());
            }
            if cgroup.contains("kubepods") {
                return Some("kubernetes".into());
            }
        }

        // Check DMI sys_vendor for hypervisor
        if let Some(vendor) = read_file_trimmed("/sys/class/dmi/id/sys_vendor") {
            let lower = vendor.to_lowercase();
            if lower.contains("qemu") || lower.contains("kvm") {
                return Some("kvm".into());
            }
            if lower.contains("vmware") {
                return Some("vmware".into());
            }
            if lower.contains("microsoft") {
                return Some("hyperv".into());
            }
            if lower.contains("xen") {
                return Some("xen".into());
            }
            if lower.contains("innotek") || lower.contains("virtualbox") {
                return Some("virtualbox".into());
            }
        }

        // Check for WSL
        if let Some(version) = read_file_trimmed("/proc/version") {
            if version.to_lowercase().contains("microsoft") {
                return Some("wsl".into());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(model) =
            read_windows_registry("HARDWARE\\DESCRIPTION\\System\\BIOS", "SystemProductName")
        {
            let lower = model.to_lowercase();
            if lower.contains("virtual machine") || lower.contains("hyper-v") {
                return Some("hyperv".into());
            }
            if lower.contains("vmware") {
                return Some("vmware".into());
            }
            if lower.contains("virtualbox") {
                return Some("virtualbox".into());
            }
        }
    }

    None
}

fn detect_is_container() -> bool {
    if std::path::Path::new("/.dockerenv").exists() {
        return true;
    }
    if std::path::Path::new("/run/.containerenv").exists() {
        return true;
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(cgroup) = read_file_trimmed("/proc/1/cgroup") {
            if cgroup.contains("docker")
                || cgroup.contains("lxc")
                || cgroup.contains("kubepods")
                || cgroup.contains("containerd")
            {
                return true;
            }
        }
        // Check for container environment variable
        if std::env::var("container").is_ok() {
            return true;
        }
    }

    false
}

fn detect_timezone() -> Option<String> {
    // Check TZ env var first
    if let Ok(tz) = std::env::var("TZ") {
        if !tz.is_empty() {
            return Some(tz);
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Try /etc/timezone
        if let Some(tz) = read_file_trimmed("/etc/timezone") {
            return Some(tz);
        }
        // Try reading /etc/localtime symlink target
        if let Ok(target) = std::fs::read_link("/etc/localtime") {
            let target_str = target.to_string_lossy();
            if let Some(pos) = target_str.find("zoneinfo/") {
                return Some(target_str[pos + 9..].to_string());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // iana_time_zone crate could be used, but for now skip on Windows
        // since it would add another dependency
    }

    None
}

fn read_domain() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let domain = read_file_trimmed("/proc/sys/kernel/domainname")?;
        if domain == "(none)" || domain.is_empty() {
            return None;
        }
        Some(domain)
    }
    #[cfg(target_os = "windows")]
    {
        read_windows_registry(
            "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters",
            "Domain",
        )
        .filter(|d| !d.is_empty())
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

#[cfg(target_os = "windows")]
fn read_windows_registry(subkey: &str, value: &str) -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm.open_subkey(subkey).ok()?;
    let val: String = key.get_value(value).ok()?;
    if val.is_empty() { None } else { Some(val) }
}

#[cfg(test)]
mod tests {
    use super::super::{HostMetrics, HostMetricsConfig, MetricsBuffer};
    use super::HOST_INFO;
    use crate::event::metric::MetricValue;

    #[test]
    fn generates_host_info_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default()).host_info_metrics(&mut buffer);
        let metrics = buffer.metrics;

        assert_eq!(metrics.len(), 1, "Expected exactly one host_info metric");

        let metric = &metrics[0];
        assert_eq!(metric.name(), HOST_INFO);

        let tags = metric.tags().expect("host_info metric must have tags");

        // These tags should always be present on any system
        assert!(tags.contains_key("os_name"), "Missing os_name tag");
        assert!(tags.contains_key("arch"), "Missing arch tag");
        assert!(
            tags.contains_key("vector_version"),
            "Missing vector_version tag"
        );
        assert!(
            tags.contains_key("is_container"),
            "Missing is_container tag"
        );
    }

    #[test]
    fn host_info_is_gauge() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default()).host_info_metrics(&mut buffer);

        let metric = &buffer.metrics[0];
        assert!(
            matches!(metric.value(), &MetricValue::Gauge { value } if value == 1.0),
            "host_info metric should be a gauge with value 1.0"
        );
    }

    #[test]
    fn host_info_has_collector_tag() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default()).host_info_metrics(&mut buffer);

        let metric = &buffer.metrics[0];
        let tags = metric.tags().unwrap();
        assert_eq!(
            tags.get("collector").expect("Missing collector tag"),
            "hostinfo"
        );
    }

    #[test]
    fn host_info_respects_namespace() {
        let mut buffer = MetricsBuffer::new(Some("custom".into()));
        HostMetrics::new(HostMetricsConfig::default()).host_info_metrics(&mut buffer);

        let metric = &buffer.metrics[0];
        assert_eq!(metric.namespace(), Some("custom"));
    }

    #[test]
    fn host_info_has_valid_vector_version() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default()).host_info_metrics(&mut buffer);

        let metric = &buffer.metrics[0];
        let tags = metric.tags().unwrap();
        let version = tags.get("vector_version").expect("Missing vector_version");
        assert!(
            !version.is_empty(),
            "vector_version tag should not be empty"
        );
    }

    #[test]
    fn host_info_arch_matches_platform() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default()).host_info_metrics(&mut buffer);

        let metric = &buffer.metrics[0];
        let tags = metric.tags().unwrap();
        let arch = tags.get("arch").expect("Missing arch tag");
        assert_eq!(arch, std::env::consts::ARCH);
    }

    #[test]
    fn host_info_is_container_is_boolean_string() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default()).host_info_metrics(&mut buffer);

        let metric = &buffer.metrics[0];
        let tags = metric.tags().unwrap();
        let val = tags.get("is_container").expect("Missing is_container");
        assert!(
            val == "true" || val == "false",
            "is_container should be 'true' or 'false', got '{val}'"
        );
    }

    #[test]
    fn host_info_has_host_tag() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default()).host_info_metrics(&mut buffer);

        let metric = &buffer.metrics[0];
        let tags = metric.tags().unwrap();
        assert!(
            tags.contains_key("host"),
            "host_info metric should have the standard 'host' tag"
        );
    }
}
