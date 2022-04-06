use headers::Authorization;
use http::uri::InvalidUri;
use hyper_proxy::{Custom, Intercept, Proxy, ProxyConnector};
use no_proxy::NoProxy;
use url::Url;

// suggestion of standardization coming from https://about.gitlab.com/blog/2021/01/27/we-need-to-talk-no-proxy/
fn from_env(key: &str) -> Option<String> {
    // use lowercase first and the uppercase
    std::env::var(key.to_lowercase())
        .ok()
        .or_else(|| std::env::var(key.to_uppercase()).ok())
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, PartialEq, Eq)]
pub struct NoProxyInterceptor(NoProxy);

impl NoProxyInterceptor {
    fn intercept(self, expected_scheme: &'static str) -> Intercept {
        Intercept::Custom(Custom::from(
            move |scheme: Option<&str>, host: Option<&str>, port: Option<u16>| {
                if scheme.is_some() && scheme != Some(expected_scheme) {
                    return false;
                }
                let matches = host.map_or(false, |host| {
                    self.0.matches(host)
                        || port.map_or(false, |port| {
                            let url = format!("{}:{}", host, port);
                            self.0.matches(&url)
                        })
                });
                // only intercept those that don't match
                !matches
            },
        ))
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProxyConfig {
    #[serde(
        default = "ProxyConfig::default_enabled",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub enabled: bool,
    #[serde(default)]
    pub http: Option<String>,
    #[serde(default)]
    pub https: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
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

    fn interceptor(&self) -> NoProxyInterceptor {
        NoProxyInterceptor(self.no_proxy.clone())
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

    fn build_proxy(
        &self,
        proxy_scheme: &'static str,
        proxy_url: &Option<String>,
    ) -> Result<Option<Proxy>, InvalidUri> {
        proxy_url
            .as_ref()
            .map(|url| {
                url.parse().map(|parsed| {
                    let mut proxy = Proxy::new(self.interceptor().intercept(proxy_scheme), parsed);
                    if let Ok(authority) = Url::parse(url) {
                        if let Some(password) = authority.password() {
                            proxy.set_authorization(Authorization::basic(
                                authority.username(),
                                password,
                            ));
                        }
                    }
                    proxy
                })
            })
            .transpose()
    }

    fn http_proxy(&self) -> Result<Option<Proxy>, InvalidUri> {
        self.build_proxy("http", &self.http)
    }

    fn https_proxy(&self) -> Result<Option<Proxy>, InvalidUri> {
        self.build_proxy("https", &self.https)
    }

    /// Install the [`ProxyConnector<C>`] for this `ProxyConfig`
    ///
    /// # Errors
    ///
    /// Function will error if passed `ProxyConnector` has a faulty URI.
    pub fn configure<C>(&self, connector: &mut ProxyConnector<C>) -> Result<(), InvalidUri> {
        if self.enabled {
            if let Some(proxy) = self.http_proxy()? {
                connector.add_proxy(proxy);
            }
            if let Some(proxy) = self.https_proxy()? {
                connector.add_proxy(proxy);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use base64::encode;
    use env_test_util::TempEnvVar;
    use http::{HeaderValue, Uri};

    use super::*;

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

    #[test]
    fn build_proxy() {
        let config = ProxyConfig {
            http: Some("http://1.2.3.4:5678".into()),
            https: Some("https://2.3.4.5:9876".into()),
            ..Default::default()
        };
        let first = config
            .http_proxy()
            .expect("should not be an error")
            .expect("should not be None");
        let second = config
            .https_proxy()
            .expect("should not be an error")
            .expect("should not be None");

        assert_eq!(
            Some(first.uri()),
            Uri::try_from("http://1.2.3.4:5678").as_ref().ok()
        );
        assert_eq!(
            Some(second.uri()),
            Uri::try_from("https://2.3.4.5:9876").as_ref().ok()
        );
    }

    #[test]
    fn build_proxy_with_basic_authorization() {
        let config = ProxyConfig {
            http: Some("http://user:pass@1.2.3.4:5678".into()),
            https: Some("https://user:pass@2.3.4.5:9876".into()),
            ..Default::default()
        };
        let first = config
            .http_proxy()
            .expect("should not be an error")
            .expect("should not be None");
        let second = config
            .https_proxy()
            .expect("should not be an error")
            .expect("should not be None");
        let encoded_header = format!("Basic {}", encode("user:pass"));
        let expected_header_value = HeaderValue::from_str(encoded_header.as_str());

        assert_eq!(
            Some(first.uri()),
            Uri::try_from("http://user:pass@1.2.3.4:5678").as_ref().ok()
        );
        assert_eq!(
            first.headers().get("authorization"),
            expected_header_value.as_ref().ok()
        );
        assert_eq!(
            Some(second.uri()),
            Uri::try_from("https://user:pass@2.3.4.5:9876")
                .as_ref()
                .ok()
        );
        assert_eq!(
            second.headers().get("authorization"),
            expected_header_value.as_ref().ok()
        );
    }
}
