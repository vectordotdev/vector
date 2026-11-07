use vector_config::configurable_component;

use crate::serde::is_default;

mod no_proxy;
pub use no_proxy::NoProxy;

// suggestion of standardization coming from https://about.gitlab.com/blog/2021/01/27/we-need-to-talk-no-proxy/
fn from_env(key: &str) -> Option<String> {
    // use lowercase first and the uppercase
    std::env::var(key.to_lowercase())
        .ok()
        .or_else(|| std::env::var(key.to_uppercase()).ok())
}

/// Proxy configuration.
///
/// Configure to proxy traffic through an HTTP(S) proxy when making external requests.
///
/// Similar to common proxy configuration convention, you can set different proxies
/// to use based on the type of traffic being proxied. You can also set specific hosts that
/// should not be proxied.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProxyConfig {
    /// Enables proxying support.
    #[serde(
        default = "ProxyConfig::default_enabled",
        skip_serializing_if = "is_enabled"
    )]
    pub enabled: bool,

    /// Proxy endpoint to use when proxying HTTP traffic.
    ///
    /// Must be a valid URI string.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "http://foo.bar:3128"))]
    #[serde(default, skip_serializing_if = "is_default")]
    pub http: Option<String>,

    /// Proxy endpoint to use when proxying HTTPS traffic.
    ///
    /// Must be a valid URI string.
    #[configurable(validation(format = "uri"))]
    #[serde(default, skip_serializing_if = "is_default")]
    #[configurable(metadata(docs::examples = "http://foo.bar:3128"))]
    pub https: Option<String>,

    /// A list of hosts to avoid proxying.
    ///
    /// Multiple patterns are allowed:
    ///
    /// | Pattern             | Example match                                                               |
    /// | ------------------- | --------------------------------------------------------------------------- |
    /// | Domain names        | `example.com` matches requests to `example.com`                     |
    /// | Wildcard domains    | `.example.com` matches requests to `example.com` and its subdomains |
    /// | IP addresses        | `127.0.0.1` matches requests to `127.0.0.1`                         |
    /// | [CIDR][cidr] blocks | `192.168.0.0/16` matches requests to any IP addresses in this range     |
    /// | Splat               | `*` matches all hosts                                                   |
    ///
    /// [cidr]: https://en.wikipedia.org/wiki/Classless_Inter-Domain_Routing
    #[serde(default, skip_serializing_if = "is_default")]
    #[configurable(metadata(docs::examples = "localhost"))]
    #[configurable(metadata(docs::examples = ".foo.bar"))]
    #[configurable(metadata(docs::examples = "*"))]
    pub no_proxy: NoProxy,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            enabled: Self::default_enabled(),
            http: None,
            https: None,
            no_proxy: NoProxy::default(),
        }
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)] // Calling convention is required by serde
fn is_enabled(e: &bool) -> bool {
    e == &true
}

impl ProxyConfig {
    fn default_enabled() -> bool {
        true
    }

    pub fn from_env() -> Self {
        Self {
            enabled: true,
            http: from_env("HTTP_PROXY"),
            https: from_env("HTTPS_PROXY"),
            no_proxy: from_env("NO_PROXY").map(NoProxy::from).unwrap_or_default(),
        }
    }

    pub fn merge_with_env(global: &Self, component: &Self) -> Self {
        Self::from_env().merge(&global.merge(component))
    }

    // overrides current proxy configuration with other configuration
    // if `self` is the global config and `other` the component config,
    // if both have the `http` proxy set, the one from `other` should be kept
    #[must_use]
    pub fn merge(&self, other: &Self) -> Self {
        let no_proxy = if other.no_proxy.is_empty() {
            self.no_proxy.clone()
        } else {
            other.no_proxy.clone()
        };

        Self {
            enabled: self.enabled && other.enabled,
            http: other.http.clone().or_else(|| self.http.clone()),
            https: other.https.clone().or_else(|| self.https.clone()),
            no_proxy,
        }
    }
}

#[cfg(test)]
mod tests {
    use env_test_util::TempEnvVar;
    use proptest::prelude::*;

    use super::*;

    impl Arbitrary for ProxyConfig {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
            (
                any::<bool>(),
                any::<Option<String>>(),
                any::<Option<String>>(),
            )
                .prop_map(|(enabled, http, https)| Self {
                    enabled,
                    http,
                    https,
                    // TODO: Neither NoProxy nor IpCidr contained with in it supports proptest. Once
                    // they support proptest, add another any here.
                    no_proxy: NoProxy::default(),
                })
                .boxed()
        }
    }

    proptest! {
        #[test]
        fn encodes_and_decodes_through_yaml(config:ProxyConfig) {
            let yaml = serde_yaml::to_string(&config).expect("Could not serialize config");
            let reloaded: ProxyConfig = serde_yaml::from_str(&yaml)
                .expect("Could not deserialize config");
            assert_eq!(config, reloaded);
        }
    }

    #[test]
    fn merge_simple() {
        let first = ProxyConfig::default();
        let second = ProxyConfig {
            https: Some("https://2.3.4.5:9876".into()),
            ..Default::default()
        };
        let result = first.merge(&second);
        assert_eq!(result.http, None);
        assert_eq!(result.https, Some("https://2.3.4.5:9876".into()));
    }

    #[test]
    fn merge_fill() {
        // coming from env
        let first = ProxyConfig {
            http: Some("http://1.2.3.4:5678".into()),
            ..Default::default()
        };
        // global config
        let second = ProxyConfig {
            https: Some("https://2.3.4.5:9876".into()),
            ..Default::default()
        };
        // component config
        let third = ProxyConfig {
            no_proxy: NoProxy::from("localhost"),
            ..Default::default()
        };
        let result = first.merge(&second).merge(&third);
        assert_eq!(result.http, Some("http://1.2.3.4:5678".into()));
        assert_eq!(result.https, Some("https://2.3.4.5:9876".into()));
        assert!(result.no_proxy.matches("localhost"));
    }

    #[test]
    fn merge_override() {
        let first = ProxyConfig {
            http: Some("http://1.2.3.4:5678".into()),
            no_proxy: NoProxy::from("127.0.0.1,google.com"),
            ..Default::default()
        };
        let second = ProxyConfig {
            http: Some("http://1.2.3.4:5678".into()),
            https: Some("https://2.3.4.5:9876".into()),
            no_proxy: NoProxy::from("localhost"),
            ..Default::default()
        };
        let result = first.merge(&second);
        assert_eq!(result.http, Some("http://1.2.3.4:5678".into()));
        assert_eq!(result.https, Some("https://2.3.4.5:9876".into()));
        assert!(!result.no_proxy.matches("127.0.0.1"));
        assert!(result.no_proxy.matches("localhost"));
    }

    #[test]
    fn with_environment_variables() {
        let global_proxy = ProxyConfig {
            http: Some("http://1.2.3.4:5678".into()),
            ..Default::default()
        };
        let component_proxy = ProxyConfig {
            https: Some("https://2.3.4.5:9876".into()),
            ..Default::default()
        };
        let _http = TempEnvVar::new("HTTP_PROXY").with("http://remote.proxy");
        let _https = TempEnvVar::new("HTTPS_PROXY");
        let result = ProxyConfig::merge_with_env(&global_proxy, &component_proxy);

        assert_eq!(result.http, Some("http://1.2.3.4:5678".into()));
        assert_eq!(result.https, Some("https://2.3.4.5:9876".into()));

        // with the component proxy disabled
        let global_proxy = ProxyConfig {
            https: Some("https://2.3.4.5:9876".into()),
            ..Default::default()
        };
        let component_proxy = ProxyConfig {
            enabled: false,
            ..Default::default()
        };
        let result = ProxyConfig::merge_with_env(&global_proxy, &component_proxy);

        assert!(!result.enabled);
        assert_eq!(result.http, Some("http://remote.proxy".into()));
        assert_eq!(result.https, Some("https://2.3.4.5:9876".into()));
    }
}
