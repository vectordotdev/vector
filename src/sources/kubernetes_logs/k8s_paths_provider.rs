//! A paths provider for k8s logs.

#![deny(missing_docs)]

use std::path::{Path, PathBuf};

use k8s_openapi::api::core::v1::{Namespace, Pod};
use kube::runtime::reflector::{ObjectRef, store::Store};
use vector_lib::file_source::paths_provider::{FileOrdering, PathsProvider};

use super::path_helpers::{build_pod_logs_directory, parse_container_log_path};
use crate::kubernetes::pod_manager_logic::extract_static_pod_config_hashsum;

/// A paths provider implementation that uses the state obtained from the
/// the k8s API.
pub(super) struct K8sPathsProvider {
    pod_state: Store<Pod>,
    namespace_state: Store<Namespace>,
    include_paths: Vec<glob::Pattern>,
    exclude_paths: Vec<glob::Pattern>,
    insert_namespace_fields: bool,
}

impl K8sPathsProvider {
    /// Create a new [`K8sPathsProvider`].
    pub(super) const fn new(
        pod_state: Store<Pod>,
        namespace_state: Store<Namespace>,
        include_paths: Vec<glob::Pattern>,
        exclude_paths: Vec<glob::Pattern>,
        insert_namespace_fields: bool,
    ) -> Self {
        Self {
            pod_state,
            namespace_state,
            include_paths,
            exclude_paths,
            insert_namespace_fields,
        }
    }
}

impl PathsProvider for K8sPathsProvider {
    type IntoIter = Vec<PathBuf>;

    fn file_ordering(&self, path: &Path) -> Option<FileOrdering> {
        let path = parse_container_log_path(path.to_str()?)?;
        Some(FileOrdering {
            group: path.attempt.into(),
            rotation: path.rotation.map(str::to_owned),
        })
    }

    fn paths(&self) -> Vec<PathBuf> {
        let state = self.pod_state.state();

        state
            .into_iter()
            // filter out pods where we haven't fetched the namespace metadata yet
            // they will be picked up on a later run
            // Only check namespace metadata if insert_namespace_fields is enabled
            .filter(|pod| {
                if !self.insert_namespace_fields {
                    // Skip namespace metadata check when namespace fields are disabled
                    return true;
                }
                trace!(message = "Verifying Namespace metadata for pod.", pod = ?pod.metadata.name);
                if let Some(namespace) = pod.metadata.namespace.as_ref() {
                    self.namespace_state
                        .get(&ObjectRef::<Namespace>::new(namespace))
                        .is_some()
                } else {
                    false
                }
            })
            .flat_map(|pod| {
                trace!(message = "Providing log paths for pod.", pod = ?pod.metadata.name);
                let paths_iter = list_pod_log_paths(real_glob, pod.as_ref());
                filter_paths(
                    filter_paths(paths_iter, &self.include_paths, true),
                    &self.exclude_paths,
                    false,
                )
                .collect::<Vec<_>>()
            })
            .collect()
    }
}

/// This function takes a `Pod` resource and returns the path to where the logs
/// for the said `Pod` are expected to be found.
///
/// In the common case, the effective path is built using the `namespace`,
/// `name` and `uid` of the Pod. However, there's a special case for
/// `Static Pod`s: they keep their logs at the path that consists of config
/// hashsum instead of the `Pod` `uid`. The reason for this is `kubelet` is
/// locally authoritative over those `Pod`s, and the API only has
/// `Monitor Pod`s - the "dummy" entries useful for discovery and association.
/// Their UIDs are generated at the Kubernetes API side, and do not represent
/// the actual config hashsum as one would expect.
///
/// To work around this, we use the mirror pod annotations (if any) to obtain
/// the effective config hashsum, see the `extract_static_pod_config_hashsum`
/// function that does this.
///
/// See <https://github.com/vectordotdev/vector/issues/6001>
/// See <https://github.com/kubernetes/kubernetes/blob/ef3337a443b402756c9f0bfb1f844b1b45ce289d/pkg/kubelet/pod/pod_manager.go#L30-L44>
/// See <https://github.com/kubernetes/kubernetes/blob/cea1d4e20b4a7886d8ff65f34c6d4f95efcb4742/pkg/kubelet/pod/mirror_client.go#L80-L81>
fn extract_pod_logs_directory(pod: &Pod) -> Option<PathBuf> {
    let metadata = &pod.metadata;
    let namespace = metadata.namespace.as_ref()?;
    let name = metadata.name.as_ref()?;

    let uid = if let Some(static_pod_config_hashsum) = extract_static_pod_config_hashsum(metadata) {
        // If there's a static pod config hashsum - use it instead of uid.
        static_pod_config_hashsum
    } else {
        // In the common case - just fallback to the real pod uid.
        metadata.uid.as_ref()?
    };

    Some(build_pod_logs_directory(namespace, name, uid))
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
            .map(str::trim)
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
                &[dir, "*/*.log*"].join("/"),
            );

            // Extract the containers to exclude, then build patterns from them
            // and cache the results into a Vec.
            let excluded_containers = extract_excluded_containers_for_pod(pod);
            let exclusion_patterns: Vec<_> =
                build_container_exclusion_patterns(dir, excluded_containers).collect();

            // Return paths filtered with container exclusion.
            filter_paths(path_iter, exclusion_patterns, false)
        })
}

fn real_glob(pattern: &str) -> impl Iterator<Item = PathBuf> + use<> {
    glob::glob_with(
        pattern,
        glob::MatchOptions {
            require_literal_separator: true,
            ..Default::default()
        },
    )
    .expect("the pattern is supposed to always be correct")
    .flat_map(IntoIterator::into_iter)
}

fn filter_paths<'a>(
    iter: impl Iterator<Item = PathBuf> + 'a,
    patterns: impl AsRef<[glob::Pattern]> + 'a,
    include: bool,
) -> impl Iterator<Item = PathBuf> + 'a {
    iter.filter(move |path| {
        let m = patterns.as_ref().iter().any(|pattern| {
            pattern.matches_path_with(
                path,
                glob::MatchOptions {
                    require_literal_separator: true,
                    ..Default::default()
                },
            )
        });
        if include { m } else { !m }
    })
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use bytes::Bytes;
    #[cfg(target_os = "linux")]
    use futures::{
        FutureExt as _, StreamExt as _,
        channel::{mpsc, oneshot},
    };
    use std::path::PathBuf;
    #[cfg(target_os = "linux")]
    use std::{
        fs,
        io::Write as _,
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    #[cfg(target_os = "linux")]
    use vector_lib::file_source_common::{
        FingerprintStrategy, Fingerprinter, ReadFrom, checkpointer::Checkpointer,
    };

    use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::ObjectMeta};

    use super::{
        build_container_exclusion_patterns, extract_excluded_containers_for_pod,
        extract_pod_logs_directory, filter_paths, list_pod_log_paths,
    };

    #[cfg(target_os = "linux")]
    struct RotationPaths {
        directory: PathBuf,
        kubernetes: super::K8sPathsProvider,
        rotate_during_discovery: Option<std::sync::atomic::AtomicUsize>,
    }

    #[cfg(target_os = "linux")]
    impl vector_lib::file_source::paths_provider::PathsProvider for RotationPaths {
        type IntoIter = Vec<PathBuf>;

        fn paths(&self) -> Self::IntoIter {
            let rotate = self
                .rotate_during_discovery
                .as_ref()
                .is_some_and(|scans| scans.fetch_add(1, Ordering::SeqCst) == 1);
            let current = self.directory.join("busy/0.log");
            if rotate {
                fs::write(&current, "prefix\n").unwrap();
            }
            let paths = glob::glob(self.directory.join("*/*.log*").to_str().unwrap())
                .unwrap()
                .map(Result::unwrap)
                .collect();
            if rotate {
                fs::rename(&current, self.directory.join("busy/0.log.20261124-190000")).unwrap();
                fs::write(current, "suffix\n").unwrap();
            }
            paths
        }

        fn file_ordering(
            &self,
            path: &std::path::Path,
        ) -> Option<vector_lib::file_source::paths_provider::FileOrdering> {
            self.kubernetes.file_ordering(path)
        }
    }

    #[cfg(target_os = "linux")]
    fn rotation_file_server(
        directory: &std::path::Path,
        data: &std::path::Path,
    ) -> vector_lib::file_source::file_server::FileServer<
        RotationPaths,
        crate::internal_events::FileSourceInternalEventsEmitter,
    > {
        vector_lib::file_source::file_server::FileServer {
            paths_provider: RotationPaths {
                directory: directory.to_owned(),
                rotate_during_discovery: None,
                kubernetes: super::K8sPathsProvider::new(
                    kube::runtime::reflector::store::Writer::default().as_reader(),
                    kube::runtime::reflector::store::Writer::default().as_reader(),
                    vec![],
                    vec![],
                    false,
                ),
            },
            max_read_bytes: 1,
            ignore_checkpoints: false,
            read_from: ReadFrom::Beginning,
            ignore_before: None,
            max_line_bytes: 1024,
            line_delimiter: Bytes::from_static(b"\n"),
            data_dir: data.to_owned(),
            glob_minimum_cooldown: Duration::from_millis(10),
            fingerprinter: Fingerprinter::new(
                FingerprintStrategy::FirstLinesChecksum {
                    ignored_header_bytes: 0,
                    lines: 1,
                },
                1024,
                true,
            ),
            oldest_first: false,
            remove_after: None,
            emitter: crate::internal_events::FileSourceInternalEventsEmitter {
                include_file_metric_tag: false,
            },
            rotate_wait: Duration::from_mins(1),
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn rotation_during_discovery_finds_predecessor_before_reading() {
        let directory = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("busy")).unwrap();
        fs::create_dir(directory.path().join("quiet")).unwrap();
        fs::write(directory.path().join("quiet/0.log"), "quiet\n").unwrap();
        let mut server = rotation_file_server(directory.path(), data.path());
        server.paths_provider.rotate_during_discovery = Some(AtomicUsize::new(0));
        let (sender, mut receiver) = mpsc::channel(2);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let shutdown = shutdown_rx.map(|_| ()).shared();
        let task = tokio::spawn(server.run(
            sender,
            shutdown.clone(),
            shutdown,
            Checkpointer::new(data.path()),
        ));
        let lines = receiver
            .by_ref()
            .flat_map(futures::stream::iter)
            .take(3)
            .collect::<Vec<_>>();
        let lines = tokio::time::timeout(Duration::from_secs(5), lines)
            .await
            .unwrap();
        shutdown_tx.send(()).unwrap();
        task.await.unwrap().unwrap();
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_ref())
                .collect::<Vec<_>>(),
            vec![b"quiet".as_slice(), b"prefix", b"suffix"]
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn rotated_files_preserve_order_and_share_read_time() {
        let directory = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("busy")).unwrap();
        fs::create_dir(directory.path().join("quiet")).unwrap();
        // Create the successor first to exercise ordering independently of inode creation time.
        fs::write(directory.path().join("busy/0.log"), "suffix\n").unwrap();
        fs::write(
            directory.path().join("busy/0.log.20261124-190000"),
            "prefix one\nprefix two\n",
        )
        .unwrap();
        fs::write(directory.path().join("quiet/0.log"), "quiet\n").unwrap();
        let server = rotation_file_server(directory.path(), data.path());
        let (sender, mut receiver) = mpsc::channel(2);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let shutdown = shutdown_rx.map(|_| ()).shared();
        let task = tokio::spawn(server.run(
            sender,
            shutdown.clone(),
            shutdown,
            Checkpointer::new(data.path()),
        ));
        let lines = receiver
            .by_ref()
            .flat_map(futures::stream::iter)
            .take(4)
            .collect::<Vec<_>>();
        let lines = tokio::time::timeout(Duration::from_secs(5), lines)
            .await
            .unwrap();
        shutdown_tx.send(()).unwrap();
        task.await.unwrap().unwrap();
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_ref())
                .collect::<Vec<_>>(),
            vec![b"prefix one".as_slice(), b"quiet", b"prefix two", b"suffix"]
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn rotation_waits_for_late_predecessor_writes() {
        let directory = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("busy")).unwrap();
        fs::create_dir(directory.path().join("quiet")).unwrap();
        let current = directory.path().join("busy/0.log");
        let mut writer = fs::File::create(&current).unwrap();
        writer.write_all(b"prefix\n").unwrap();
        fs::write(directory.path().join("quiet/0.log"), "quiet one\n").unwrap();
        let server = rotation_file_server(directory.path(), data.path());
        let (sender, mut receiver) = mpsc::channel(2);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let shutdown = shutdown_rx.map(|_| ()).shared();
        let task = tokio::spawn(server.run(
            sender,
            shutdown.clone(),
            shutdown,
            Checkpointer::new(data.path()),
        ));
        let first = tokio::time::timeout(Duration::from_secs(5), receiver.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            first
                .iter()
                .map(|line| line.text.as_ref())
                .collect::<Vec<_>>(),
            vec![b"prefix".as_slice(), b"quiet one"]
        );
        // An empty polling batch confirms that the reader has reached the live file's EOF.
        while !tokio::time::timeout(Duration::from_secs(5), receiver.next())
            .await
            .unwrap()
            .unwrap()
            .is_empty()
        {}
        fs::rename(
            &current,
            directory.path().join("busy/0.log.20261124-190000"),
        )
        .unwrap();
        fs::write(&current, "suffix\n").unwrap();
        fs::create_dir(directory.path().join("new-quiet")).unwrap();
        fs::write(directory.path().join("new-quiet/0.log"), "quiet two\n").unwrap();
        loop {
            let batch = tokio::time::timeout(Duration::from_secs(5), receiver.next())
                .await
                .unwrap()
                .unwrap();
            if !batch.is_empty() {
                assert_eq!(
                    batch
                        .iter()
                        .map(|line| line.text.as_ref())
                        .collect::<Vec<_>>(),
                    vec![b"quiet two".as_slice()]
                );
                break;
            }
        }
        writer.write_all(b"late prefix\n").unwrap();
        drop(writer);
        let remaining = receiver
            .by_ref()
            .flat_map(futures::stream::iter)
            .take(2)
            .collect::<Vec<_>>();
        let remaining = tokio::time::timeout(Duration::from_secs(5), remaining)
            .await
            .unwrap();
        shutdown_tx.send(()).unwrap();
        task.await.unwrap().unwrap();
        assert_eq!(
            remaining
                .iter()
                .map(|line| line.text.as_ref())
                .collect::<Vec<_>>(),
            vec![b"late prefix".as_slice(), b"suffix"]
        );
    }

    #[test]
    fn test_extract_pod_logs_directory() {
        let cases = vec![
            // Empty pod.
            (Pod::default(), None),
            // Happy path.
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
            // No uid.
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
            // No name.
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
            // No namespace.
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
            // Static pod config hashsum as uid.
            (
                Pod {
                    metadata: ObjectMeta {
                        namespace: Some("sandbox0-ns".to_owned()),
                        name: Some("sandbox0-name".to_owned()),
                        uid: Some("sandbox0-uid".to_owned()),
                        annotations: Some(
                            vec![(
                                "kubernetes.io/config.mirror".to_owned(),
                                "sandbox0-config-hashsum".to_owned(),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                        ..ObjectMeta::default()
                    },
                    ..Pod::default()
                },
                Some("/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-config-hashsum"),
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
                    "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/*/*.log*",
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
                    "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/*/*.log*",
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
            assert_eq!(actual_paths, expected_paths);
        }
    }

    #[test]
    fn test_exclude_paths() {
        let cases = vec![
            // No exclusion pattern allows everything.
            (
                vec![
                    "/var/log/pods/a.log",
                    "/var/log/pods/b.log",
                    "/var/log/pods/c.log.foo",
                    "/var/log/pods/d.logbar",
                ],
                vec![],
                vec![
                    "/var/log/pods/a.log",
                    "/var/log/pods/b.log",
                    "/var/log/pods/c.log.foo",
                    "/var/log/pods/d.logbar",
                ],
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
                filter_paths(input_paths.into_iter().map(Into::into), &patterns, false).collect();
            let expected_paths: Vec<_> = expected_paths.into_iter().map(PathBuf::from).collect();
            assert_eq!(
                actual_paths, expected_paths,
                "failed for patterns {str_patterns:?}"
            );
        }
    }

    #[test]
    fn test_include_paths() {
        let cases = vec![
            (
                vec![
                    "/var/log/pods/a.log",
                    "/var/log/pods/b.log",
                    "/var/log/pods/c.log.foo",
                    "/var/log/pods/d.logbar",
                    "/tmp/foo",
                ],
                vec!["/var/log/pods/*"],
                vec![
                    "/var/log/pods/a.log",
                    "/var/log/pods/b.log",
                    "/var/log/pods/c.log.foo",
                    "/var/log/pods/d.logbar",
                ],
            ),
            (
                vec![
                    "/var/log/pods/a.log",
                    "/var/log/pods/b.log",
                    "/var/log/pods/c.log.foo",
                    "/var/log/pods/d.logbar",
                ],
                vec!["/tmp/*"],
                vec![],
            ),
            (
                vec!["/var/log/pods/a.log", "/tmp/foo"],
                vec!["**/*"],
                vec!["/var/log/pods/a.log", "/tmp/foo"],
            ),
        ];

        for (input_paths, str_patterns, expected_paths) in cases {
            let patterns: Vec<_> = str_patterns
                .iter()
                .map(|pattern| glob::Pattern::new(pattern).unwrap())
                .collect();
            let actual_paths: Vec<_> =
                filter_paths(input_paths.into_iter().map(Into::into), &patterns, true).collect();
            let expected_paths: Vec<_> = expected_paths.into_iter().map(PathBuf::from).collect();
            assert_eq!(
                actual_paths, expected_paths,
                "failed for patterns {str_patterns:?}"
            );
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
                "failed for dir {pod_logs_dir:?} and containers {containers:?}",
            );
        }
    }
}
