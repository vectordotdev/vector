use crate::kubernetes as k8s;
use k8s_openapi::WatchOptional;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum Config {
    Path(String),
    NamespacedResources {
        resource_group: String,
        resource_version: String,
        resource_kind_plural: String,
        namespace: String,
    },
    NonNamespacedResources {
        resource_group: String,
        resource_version: String,
        resource_kind_plural: String,
    },
}

impl Default for Config {
    fn default() -> Self {
        Config::NonNamespacedResources {
            resource_group: "core".to_owned(),
            resource_version: "v1".to_owned(),
            resource_kind_plural: "pods".to_owned(),
        }
    }
}

impl Config {
    fn path(&self) -> String {
        match self {
            Config::Path(ref path) => path.clone(),
            Config::NamespacedResources {
                resource_group,
                resource_version,
                resource_kind_plural,
                namespace,
            } => format!(
                "{}/{}/namespaces/{}/{}",
                api_root(resource_group),
                resource_version,
                namespace,
                resource_kind_plural,
            ),
            Config::NonNamespacedResources {
                resource_group,
                resource_version,
                resource_kind_plural,
            } => format!(
                "{}/{}/{}",
                api_root(resource_group),
                resource_version,
                resource_kind_plural
            ),
        }
    }
}

fn api_root(api_group: &str) -> Cow<'static, str> {
    if api_group == "core" {
        return Cow::Borrowed("/api");
    }
    Cow::Owned(format!("/apis/{}", api_group))
}

#[derive(Debug)]
pub struct Builder {
    path: String,
}

impl From<&Config> for Builder {
    fn from(config: &Config) -> Self {
        Self {
            path: config.path(),
        }
    }
}

impl k8s::WatchRequestBuilder for Builder {
    type Object = k8s::any_resource::AnyResource;

    fn build<'a>(
        &self,
        watch_optional: WatchOptional<'a>,
    ) -> Result<http::Request<Vec<u8>>, k8s_openapi::RequestError> {
        let url = format!("{}?", self.path);
        let mut query_pairs = k8s_openapi::url::form_urlencoded::Serializer::new(url);
        watch_optional.__serialize(&mut query_pairs);
        let url = query_pairs.finish();

        let request = http::Request::get(url);
        let body = vec![];
        request.body(body).map_err(k8s_openapi::RequestError::Http)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s::WatchRequestBuilder;

    #[test]
    fn test_config_path() {
        let cases = vec![
            // Raw path, watch `Pod`s across all namespaces.
            (Config::Path("/api/v1/pods".to_owned()), "/api/v1/pods"),
            // Raw path, watch `Pod`s in the `default` namespace.
            (
                Config::Path("/api/v1/namespaces/default/pods".to_owned()),
                "/api/v1/namespaces/default/pods",
            ),
            // Watch `Pod`s in the `default` namespace.
            (
                Config::NamespacedResources {
                    resource_group: "core".to_owned(),
                    resource_version: "v1".to_owned(),
                    resource_kind_plural: "pods".to_owned(),
                    namespace: "default".to_owned(),
                },
                "/api/v1/namespaces/default/pods",
            ),
            // Watch `Pod`s in all namespaces.
            (
                Config::NonNamespacedResources {
                    resource_group: "core".to_owned(),
                    resource_version: "v1".to_owned(),
                    resource_kind_plural: "pods".to_owned(),
                },
                "/api/v1/pods",
            ),
            // Watch `DaemonSet`s in the `default` namespace.
            (
                Config::NamespacedResources {
                    resource_group: "apps".to_owned(),
                    resource_version: "v1".to_owned(),
                    resource_kind_plural: "daemonsets".to_owned(),
                    namespace: "default".to_owned(),
                },
                "/apis/apps/v1/namespaces/default/daemonsets",
            ),
            // Watch `DaemonSet`s in all namespaces.
            (
                Config::NonNamespacedResources {
                    resource_group: "apps".to_owned(),
                    resource_version: "v1".to_owned(),
                    resource_kind_plural: "daemonsets".to_owned(),
                },
                "/apis/apps/v1/daemonsets",
            ),
        ];

        for (input_config, expected_path) in cases {
            assert_eq!(input_config.path(), expected_path);
        }
    }

    #[test]
    fn test_builder() {
        let cases = vec![
            // Watch `Pod`s across all namespaces.
            (
                "/api/v1/pods",
                vec![
                    // Start a new watch sequence.
                    (WatchOptional::default(), "/api/v1/pods?&watch=true"),
                    // Resume a watch sequence.
                    (
                        WatchOptional {
                            resource_version: Some("123"),
                            ..Default::default()
                        },
                        "/api/v1/pods?&resourceVersion=123&watch=true",
                    ),
                ],
            ),
            // Watch `DaemonSet`s across all namespaces.
            (
                "/apis/apps/v1/daemonsets",
                vec![
                    // Start a new watch sequence.
                    (
                        WatchOptional::default(),
                        "/apis/apps/v1/daemonsets?&watch=true",
                    ),
                    // Resume a watch sequence.
                    (
                        WatchOptional {
                            resource_version: Some("123"),
                            ..Default::default()
                        },
                        "/apis/apps/v1/daemonsets?&resourceVersion=123&watch=true",
                    ),
                ],
            ),
            // Watch `Pod`s in the `default` namespace.
            (
                "/api/v1/namespaces/default/pods",
                vec![
                    // Start a new watch sequence.
                    (
                        WatchOptional::default(),
                        "/api/v1/namespaces/default/pods?&watch=true",
                    ),
                ],
            ),
        ];

        for (path, subcases) in cases {
            let builder = Builder {
                path: path.to_owned(),
            };
            for (input_watch_options, expected_request_url) in subcases {
                let request = builder
                    .build(input_watch_options)
                    .expect("failed to build a request");

                assert_eq!(request.method(), http::Method::GET);
                assert_eq!(request.uri().to_string(), expected_request_url);
            }
        }
    }
}
