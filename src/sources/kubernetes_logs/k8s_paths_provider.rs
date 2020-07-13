//! A paths provider for k8s logs.

#![deny(missing_docs)]

use super::path_helpers::build_pod_logs_directory;
use evmap10::ReadHandle;
use file_source::paths_provider::PathsProvider;
use k8s_openapi::api::core::v1::Pod;
use std::path::PathBuf;

/// A paths provider implementation that uses the state obtained from the
/// the k8s API.
pub struct K8sPathsProvider {
    pods_state_reader: ReadHandle<String, k8s_runtime::state::evmap::Value<Pod>>,
}

impl K8sPathsProvider {
    /// Create a new [`K8sPathsProvider`].
    pub fn new(pods_state_reader: ReadHandle<String, k8s_runtime::state::evmap::Value<Pod>>) -> Self {
        Self { pods_state_reader }
    }
}

impl PathsProvider for K8sPathsProvider {
    type IntoIter = Vec<PathBuf>;

    fn paths(&self) -> Vec<PathBuf> {
        let read_ref = match self.pods_state_reader.read() {
            Some(v) => v,
            None => {
                // The state is not initialized or gone, fallback to using an
                // empty array.
                // TODO: consider `panic`ing here instead - fail-fast appoach
                // is always better if possible, but it's not clear if it's
                // a sane strategy here.
                warn!(message = "unable to read the state of the pods");
                return Vec::new();
            }
        };

        read_ref
            .into_iter()
            .flat_map(|(uid, values)| {
                let pod = values
                    .get_one()
                    .expect("we are supposed to be working with single-item values only");
                trace!(message = "providing log paths for pod", ?uid);
                list_pod_log_paths(real_glob, pod)
            })
            .collect()
    }
}

fn extract_pod_logs_directory(pod: &Pod) -> Option<PathBuf> {
    let metadata = &pod.metadata;
    let namespace = metadata.namespace.as_ref()?;
    let name = metadata.name.as_ref()?;
    let uid = metadata.uid.as_ref()?;
    Some(build_pod_logs_directory(&namespace, &name, &uid))
}

fn list_pod_log_paths<'a, G, GI>(mut glob_impl: G, pod: &Pod) -> impl Iterator<Item = PathBuf> + 'a
where
    G: FnMut(&str) -> GI + 'a,
    GI: Iterator<Item = PathBuf> + 'a,
{
    extract_pod_logs_directory(pod)
        .into_iter()
        .flat_map(move |dir| {
            glob_impl(
                // We seek to match the paths like
                // `<pod_logs_dir>/<container_name>/<n>.log` - paths managed by
                // the `kubelet` as part of Kubernetes core logging
                // architecture.
                // In some setups, there will also be paths like
                // `<pod_logs_dir>/<hash>.log` - those we want to skip.
                &[
                    dir.to_str()
                        .expect("non-utf8 path to pod logs dir is not supported"),
                    "*/*.log",
                ]
                .join("/"),
            )
        })
}

fn real_glob(pattern: &str) -> impl Iterator<Item = PathBuf> {
    glob::glob(pattern)
        .expect("the pattern is supposed to always be correct")
        .flat_map(|paths| paths.into_iter())
}

#[cfg(test)]
mod tests {
    use super::{extract_pod_logs_directory, list_pod_log_paths};
    use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::ObjectMeta};
    use std::path::PathBuf;

    #[test]
    fn test_extract_pod_logs_directory() {
        let cases = vec![
            (Pod::default(), None),
            (
                Pod {
                    metadata: ObjectMeta {
                        namespace: Some("sandbox0-ns".to_owned()),
                        name: Some("sandbox0-name".to_owned()),
                        uid: Some("sandbox0-uid".to_owned()),
                        ..ObjectMeta::default()
                    },
                    ..Pod::default()
                },
                Some("/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid"),
            ),
            (
                Pod {
                    metadata: ObjectMeta {
                        namespace: Some("sandbox0-ns".to_owned()),
                        name: Some("sandbox0-name".to_owned()),
                        ..ObjectMeta::default()
                    },
                    ..Pod::default()
                },
                None,
            ),
            (
                Pod {
                    metadata: ObjectMeta {
                        namespace: Some("sandbox0-ns".to_owned()),
                        uid: Some("sandbox0-uid".to_owned()),
                        ..ObjectMeta::default()
                    },
                    ..Pod::default()
                },
                None,
            ),
            (
                Pod {
                    metadata: ObjectMeta {
                        name: Some("sandbox0-name".to_owned()),
                        uid: Some("sandbox0-uid".to_owned()),
                        ..ObjectMeta::default()
                    },
                    ..Pod::default()
                },
                None,
            ),
        ];

        for (pod, expected) in cases {
            assert_eq!(
                extract_pod_logs_directory(&pod),
                expected.map(PathBuf::from)
            );
        }
    }

    #[test]
    fn test_list_pod_log_paths() {
        let cases = vec![
            // Pod exists and has some containers that write logs.
            (
                Pod {
                    metadata: ObjectMeta {
                        namespace: Some("sandbox0-ns".to_owned()),
                        name: Some("sandbox0-name".to_owned()),
                        uid: Some("sandbox0-uid".to_owned()),
                        ..ObjectMeta::default()
                    },
                    ..Pod::default()
                },
                // Calls to the glob mock.
                vec![(
                    // The pattern to expect at the mock.
                    "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/*/*.log",
                    // The paths to return from the mock.
                    vec![
                        "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container1/qwe.log",
                        "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container2/qwe.log",
                        "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container3/qwe.log",
                    ],
                )],
                // Expected result.
                vec![
                    "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container1/qwe.log",
                    "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container2/qwe.log",
                    "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container3/qwe.log",
                ],
            ),
            // Pod doesn't have the metadata set.
            (Pod::default(), vec![], vec![]),
            // Pod has proper metadata, but doesn't have log files.
            (
                Pod {
                    metadata: ObjectMeta {
                        namespace: Some("sandbox0-ns".to_owned()),
                        name: Some("sandbox0-name".to_owned()),
                        uid: Some("sandbox0-uid".to_owned()),
                        ..ObjectMeta::default()
                    },
                    ..Pod::default()
                },
                vec![(
                    "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/*/*.log",
                    vec![],
                )],
                vec![],
            ),
        ];

        for (pod, expected_calls, expected_paths) in cases {
            // Prepare the mock fn.
            let mut expected_calls = expected_calls.into_iter();
            let mock_glob = move |pattern: &str| {
                let (expected_pattern, paths_to_return) = expected_calls
                    .next()
                    .expect("implementation did a call that wasn't expected");

                assert_eq!(pattern, expected_pattern);
                paths_to_return.into_iter().map(PathBuf::from)
            };

            let actual_paths: Vec<_> = list_pod_log_paths(mock_glob, &pod).collect();
            let expeced_paths: Vec<_> = expected_paths.into_iter().map(PathBuf::from).collect();
            assert_eq!(actual_paths, expeced_paths)
        }
    }
}
