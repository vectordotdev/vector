//! Simple helpers for building and parsing k8s paths.
//!
//! Loosely based on <https://github.com/kubernetes/kubernetes/blob/31305966789525fca49ec26c289e565467d1f1c4/pkg/kubelet/kuberuntime/helpers.go>.

#![deny(missing_docs)]

use std::path::PathBuf;

/// The root directory for pod logs.
const K8S_LOGS_DIR: &str = "/var/log/pods";

/// The delimiter used in the log path.
const LOG_PATH_DELIMITER: &str = "_";

/// Builds absolute log directory path for a pod sandbox.
///
/// Based on <https://github.com/kubernetes/kubernetes/blob/31305966789525fca49ec26c289e565467d1f1c4/pkg/kubelet/kuberuntime/helpers.go#L178>
pub(super) fn build_pod_logs_directory(
    pod_namespace: &str,
    pod_name: &str,
    pod_uid: &str,
) -> PathBuf {
    [
        K8S_LOGS_DIR,
        &[pod_namespace, pod_name, pod_uid].join(LOG_PATH_DELIMITER),
    ]
    .join("/")
    .into()
}

/// Parses pod log file path and returns the log file info.
///
/// Assumes the input is a valid pod log file name.
///
/// Inspired by <https://github.com/kubernetes/kubernetes/blob/31305966789525fca49ec26c289e565467d1f1c4/pkg/kubelet/kuberuntime/helpers.go#L186>
pub(super) fn parse_log_file_path(path: &str) -> Option<LogFileInfo<'_>> {
    let mut components = path.rsplit('/');

    let _log_file_name = components.next()?;
    let container_name = components.next()?;
    let pod_dir = components.next()?;

    let mut pod_dir_components = pod_dir.rsplit(LOG_PATH_DELIMITER);

    let pod_uid = pod_dir_components.next()?;
    let pod_name = pod_dir_components.next()?;
    let pod_namespace = pod_dir_components.next()?;

    Some(LogFileInfo {
        pod_namespace,
        pod_name,
        pod_uid,
        container_name,
    })
}

/// Contains the information extracted from the pod log file path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LogFileInfo<'a> {
    pub pod_namespace: &'a str,
    pub pod_name: &'a str,
    pub pod_uid: &'a str,
    pub container_name: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_pod_logs_directory() {
        let cases = vec![
            // Valid inputs.
            (
                ("sandbox0-ns", "sandbox0-name", "sandbox0-uid"),
                "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid",
            ),
            // Invalid inputs.
            (("", "", ""), "/var/log/pods/__"),
        ];

        for ((in_namespace, in_name, in_uid), expected) in cases.into_iter() {
            assert_eq!(
                build_pod_logs_directory(in_namespace, in_name, in_uid),
                PathBuf::from(expected)
            );
        }
    }

    #[test]
    fn test_parse_log_file_path() {
        let cases = vec![
            // Valid inputs.
            (
                "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/sandbox0-container0-name/1.log",
                Some(LogFileInfo {
                    pod_namespace: "sandbox0-ns",
                    pod_name: "sandbox0-name",
                    pod_uid: "sandbox0-uid",
                    container_name: "sandbox0-container0-name",
                }),
            ),
            // Invalid inputs.
            ("/var/log/pods/other", None),
            ("qwe", None),
            ("", None),
        ];

        for (input, expected) in cases.into_iter() {
            assert_eq!(parse_log_file_path(input), expected);
        }
    }
}
