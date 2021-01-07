//! A paths provider for k8s logs.

#![deny(missing_docs)]

use super::path_helpers::build_pod_logs_directory;
use crate::kubernetes as k8s;
use evmap::ReadHandle;
use file_source::paths_provider::PathsProvider;
use k8s_openapi::api::core::v1::Pod;
use std::path::PathBuf;

/// A paths provider implementation that uses the state obtained from the
/// the k8s API.
pub struct K8sPathsProvider {
    pods_state_reader: ReadHandle<String, k8s::state::evmap::Value<Pod>>,
    exclude_paths: Vec<glob::Pattern>,
}

impl K8sPathsProvider {
    /// Create a new [`K8sPathsProvider`].
    pub fn new(
        pods_state_reader: ReadHandle<String, k8s::state::evmap::Value<Pod>>,
        exclude_paths: Vec<glob::Pattern>,
    ) -> Self {
        Self {
            pods_state_reader,
            exclude_paths,
        }
    }
}

impl PathsProvider for K8sPathsProvider {
    type IntoIter = Vec<PathBuf>;
    type Error = ();

    fn paths(&self) -> Result<Self::IntoIter, Self::Error> {
        let read_ref = match self.pods_state_reader.read() {
            Some(v) => v,
            None => {
                // The state is not initialized or gone, fallback to using an
                // empty array.
                // TODO: consider `panic`ing here instead - fail-fast approach
                // is always better if possible, but it's not clear if it's
                // a sane strategy here.
                warn!(message = "Unable to read the state of the pods.");
                return Ok(Vec::new());
            }
        };

        Ok(read_ref
            .into_iter()
            .flat_map(|(uid, values)| {
                let pod = values
                    .get_one()
                    .expect("we are supposed to be working with single-item values only");
                trace!(message = "Providing log paths for pod.", uid = ?uid);
                let paths_iter = list_pod_log_paths(real_glob, pod);
                exclude_paths(paths_iter, &self.exclude_paths)
            })
            .collect())
    }
}

fn extract_pod_logs_directory(pod: &Pod) -> Option<PathBuf> {
    let metadata = &pod.metadata;
    let namespace = metadata.namespace.as_ref()?;
    let name = metadata.name.as_ref()?;
    let uid = metadata.uid.as_ref()?;
    Some(build_pod_logs_directory(&namespace, &name, &uid))
}

const CONTAINER_EXCLUSION_ANNOTATION_KEY: &str = "vector.dev/exclude-containers";

fn extract_excluded_containers_for_pod(pod: &Pod) -> impl Iterator<Item = &str> {
    let metadata = &pod.metadata;
    metadata.annotations.iter().flat_map(|annotations| {
        annotations
            .iter()
            .filter_map(|(key, value)| {
                if key != CONTAINER_EXCLUSION_ANNOTATION_KEY {
                    return None;
                }
                Some(value)
            })
            .flat_map(|containers| containers.split(','))
            .map(|container| container.trim())
    })
}

fn build_container_exclusion_patterns<'a>(
    pod_logs_dir: &'a str,
    containers: impl Iterator<Item = &'a str> + 'a,
) -> impl Iterator<Item = glob::Pattern> + 'a {
    containers.filter_map(move |container| {
        let escaped_container_name = glob::Pattern::escape(container);
        glob::Pattern::new(&[pod_logs_dir, &escaped_container_name, "**"].join("/")).ok()
    })
}

fn list_pod_log_paths<'a, G, GI>(
    mut glob_impl: G,
    pod: &'a Pod,
) -> impl Iterator<Item = PathBuf> + 'a
where
    G: FnMut(&str) -> GI + 'a,
    GI: Iterator<Item = PathBuf> + 'a,
{
    extract_pod_logs_directory(pod)
        .into_iter()
        .flat_map(move |dir| {
            let dir = dir
                .to_str()
                .expect("non-utf8 path to pod logs dir is not supported");

            // Run the glob to get a list of unfiltered paths.
            let path_iter = glob_impl(
                // We seek to match the paths like
                // `<pod_logs_dir>/<container_name>/<n>.log` - paths managed by
                // the `kubelet` as part of Kubernetes core logging
                // architecture.
                // In some setups, there will also be paths like
                // `<pod_logs_dir>/<hash>.log` - those we want to skip.
                &[dir, "*/*.log"].join("/"),
            );

            // Extract the containers to exclude, then build patters from them
            // and cache the results into a Vec.
            let excluded_containers = extract_excluded_containers_for_pod(pod);
            let exclusion_patterns: Vec<_> =
                build_container_exclusion_patterns(dir, excluded_containers).collect();

            // Return paths filtered with container exclusion.
            exclude_paths(path_iter, exclusion_patterns)
        })
}

fn real_glob(pattern: &str) -> impl Iterator<Item = PathBuf> {
    glob::glob_with(
        pattern,
        glob::MatchOptions {
            require_literal_separator: true,
            ..Default::default()
        },
    )
    .expect("the pattern is supposed to always be correct")
    .flat_map(|paths| paths.into_iter())
}

fn exclude_paths<'a>(
    iter: impl Iterator<Item = PathBuf> + 'a,
    patterns: impl AsRef<[glob::Pattern]> + 'a,
) -> impl Iterator<Item = PathBuf> + 'a {
    iter.filter(move |path| {
        !patterns.as_ref().iter().any(|pattern| {
            pattern.matches_path_with(
                path,
                glob::MatchOptions {
                    require_literal_separator: true,
                    ..Default::default()
                },
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{
        build_container_exclusion_patterns, exclude_paths, extract_excluded_containers_for_pod,
        extract_pod_logs_directory, list_pod_log_paths,
    };
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
    fn test_extract_excluded_containers_for_pod() {
        let cases = vec![
            // No annotations.
            (Pod::default(), vec![]),
            // Empty annotations.
            (
                Pod {
                    metadata: ObjectMeta {
                        annotations: Some(vec![].into_iter().collect()),
                        ..ObjectMeta::default()
                    },
                    ..Pod::default()
                },
                vec![],
            ),
            // Irrelevant annotations.
            (
                Pod {
                    metadata: ObjectMeta {
                        annotations: Some(
                            vec![("some-other-annotation".to_owned(), "some value".to_owned())]
                                .into_iter()
                                .collect(),
                        ),
                        ..ObjectMeta::default()
                    },
                    ..Pod::default()
                },
                vec![],
            ),
            // Proper annotation without spaces.
            (
                Pod {
                    metadata: ObjectMeta {
                        annotations: Some(
                            vec![(
                                super::CONTAINER_EXCLUSION_ANNOTATION_KEY.to_owned(),
                                "container1,container4".to_owned(),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                        ..ObjectMeta::default()
                    },
                    ..Pod::default()
                },
                vec!["container1", "container4"],
            ),
            // Proper annotation with spaces.
            (
                Pod {
                    metadata: ObjectMeta {
                        annotations: Some(
                            vec![(
                                super::CONTAINER_EXCLUSION_ANNOTATION_KEY.to_owned(),
                                "container1, container4".to_owned(),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                        ..ObjectMeta::default()
                    },
                    ..Pod::default()
                },
                vec!["container1", "container4"],
            ),
        ];

        for (pod, expected) in cases {
            let actual: Vec<&str> = extract_excluded_containers_for_pod(&pod).collect();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_list_pod_log_paths() {
        let cases = vec![
            // Pod exists and has some containers that write logs, and some of
            // the containers are excluded.
            (
                Pod {
                    metadata: ObjectMeta {
                        namespace: Some("sandbox0-ns".to_owned()),
                        name: Some("sandbox0-name".to_owned()),
                        uid: Some("sandbox0-uid".to_owned()),
                        annotations: Some(
                            vec![(
                                super::CONTAINER_EXCLUSION_ANNOTATION_KEY.to_owned(),
                                "excluded1,excluded2".to_owned(),
                            )]
                            .into_iter()
                            .collect(),
                        ),
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
                        "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/excluded1/qwe.log",
                        "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container3/qwe.log",
                        "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/excluded2/qwe.log",
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
            let expected_paths: Vec<_> = expected_paths.into_iter().map(PathBuf::from).collect();
            assert_eq!(actual_paths, expected_paths)
        }
    }

    #[test]
    fn test_exclude_paths() {
        let cases = vec![
            // No exclusion pattern allows everything.
            (
                vec!["/var/log/pods/a.log", "/var/log/pods/b.log"],
                vec![],
                vec!["/var/log/pods/a.log", "/var/log/pods/b.log"],
            ),
            // Test a filter that doesn't apply to anything.
            (
                vec!["/var/log/pods/a.log", "/var/log/pods/b.log"],
                vec!["notmatched"],
                vec!["/var/log/pods/a.log", "/var/log/pods/b.log"],
            ),
            // Multiple filters.
            (
                vec![
                    "/var/log/pods/a.log",
                    "/var/log/pods/b.log",
                    "/var/log/pods/c.log",
                ],
                vec!["notmatched", "**/b.log", "**/c.log"],
                vec!["/var/log/pods/a.log"],
            ),
            // Requires literal path separator (`*` does not include dirs).
            (
                vec![
                    "/var/log/pods/a.log",
                    "/var/log/pods/b.log",
                    "/var/log/pods/c.log",
                ],
                vec!["*/b.log", "**/c.log"],
                vec!["/var/log/pods/a.log", "/var/log/pods/b.log"],
            ),
            // Filtering by container name with a real-life-like file path.
            (
                vec![
                    "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container1/1.log",
                    "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container1/2.log",
                    "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container2/1.log",
                ],
                vec!["**/container1/**"],
                vec!["/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container2/1.log"],
            ),
        ];

        for (input_paths, str_patterns, expected_paths) in cases {
            let patterns: Vec<_> = str_patterns
                .iter()
                .map(|pattern| glob::Pattern::new(pattern).unwrap())
                .collect();
            let actual_paths: Vec<_> =
                exclude_paths(input_paths.into_iter().map(Into::into), &patterns).collect();
            let expected_paths: Vec<_> = expected_paths.into_iter().map(PathBuf::from).collect();
            assert_eq!(
                actual_paths, expected_paths,
                "failed for patterns {:?}",
                &str_patterns
            )
        }
    }

    #[test]
    fn test_build_container_exclusion_patterns() {
        let cases = vec![
            // No excluded containers - no exclusion patterns.
            (
                "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid",
                vec![],
                vec![],
            ),
            // Ensure the paths are concatenated correctly and look good.
            (
                "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid",
                vec!["container1", "container2"],
                vec![
                    "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container1/**",
                    "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/container2/**",
                ],
            ),
            // Ensure control characters are escaped properly.
            (
                "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid",
                vec!["*[]"],
                vec!["/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/[*][[][]]/**"],
            ),
        ];

        for (pod_logs_dir, containers, expected_patterns) in cases {
            let actual_patterns: Vec<_> =
                build_container_exclusion_patterns(pod_logs_dir, containers.clone().into_iter())
                    .collect();
            let expected_patterns: Vec<_> = expected_patterns
                .into_iter()
                .map(|pattern| glob::Pattern::new(pattern).unwrap())
                .collect();
            assert_eq!(
                actual_patterns, expected_patterns,
                "failed for dir {:?} and containers {:?}",
                &pod_logs_dir, &containers,
            )
        }
    }
}
