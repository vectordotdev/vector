//! This mod contains bits of logic related to the `kubelet` part called
//! Pod Manager internal implementation.

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

/// Extract the static pod config hashsum from the mirror pod annotations.
///
/// This part of Kubernetes changed a bit over time, so we're implementing
/// support up to 1.14, which is an MSKV at this time.
///
/// See: <https://github.com/kubernetes/kubernetes/blob/cea1d4e20b4a7886d8ff65f34c6d4f95efcb4742/pkg/kubelet/pod/mirror_client.go#L80-L81>
pub fn extract_static_pod_config_hashsum(metadata: &ObjectMeta) -> Option<&str> {
    let annotations = metadata.annotations.as_ref()?;
    annotations
        .get("kubernetes.io/config.mirror")
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_static_pod_config_hashsum() {
        let cases = vec![
            (ObjectMeta::default(), None),
            (
                ObjectMeta {
                    annotations: Some(vec![].into_iter().collect()),
                    ..ObjectMeta::default()
                },
                None,
            ),
            (
                ObjectMeta {
                    annotations: Some(
                        vec![(
                            "kubernetes.io/config.mirror".to_owned(),
                            "config-hashsum".to_owned(),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                    ..ObjectMeta::default()
                },
                Some("config-hashsum"),
            ),
            (
                ObjectMeta {
                    annotations: Some(
                        vec![
                            (
                                "kubernetes.io/config.mirror".to_owned(),
                                "config-hashsum".to_owned(),
                            ),
                            ("other".to_owned(), "value".to_owned()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..ObjectMeta::default()
                },
                Some("config-hashsum"),
            ),
        ];

        for (metadata, expected) in cases {
            assert_eq!(extract_static_pod_config_hashsum(&metadata), expected);
        }
    }
}
