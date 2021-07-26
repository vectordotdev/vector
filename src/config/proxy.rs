use http::uri::InvalidUri;
use hyper_proxy::{Custom, Intercept, Proxy, ProxyConnector};
use no_proxy::NoProxy;

fn from_env(key: &str) -> Option<String> {
    // use lowercase first and the uppercase
    std::env::var(key.to_string().to_lowercase())
        .ok()
        .or_else(|| std::env::var(key.to_uppercase()).ok())
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, PartialEq, Eq)]
pub struct NoProxyInterceptor(NoProxy);

impl NoProxyInterceptor {
    fn intercept(self, expected_scheme: &'static str) -> Intercept {
        Intercept::Custom(Custom::from(
            move |scheme: Option<&str>, host: Option<&str>, port: Option<u16>| {
                if scheme != Some(expected_scheme) {
                    return false;
                }
                let matches = if let Some(host) = host {
                    if let Some(port) = port {
                        let url = format!("{}:{}", host, port);
                        self.0.matches(&url) || self.0.matches(&host)
                    } else {
                        self.0.matches(&host)
                    }
                } else {
                    false
                };
                // only intercapt those that don't match
                !matches
            },
        ))
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, PartialEq, Eq)]
pub struct ProxyConfig {
    #[serde(
        default = "ProxyConfig::default_enabled",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub enabled: bool,
    pub http: Option<String>,
    pub https: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub no_proxy: NoProxy,
}

impl ProxyConfig {
    fn default_enabled() -> bool {
        true
    }

    pub fn from_env() -> Self {
        Self {
            enabled: true,
            http: from_env("HTTP_PROXY"),
            https: from_env("HTTP_PROXYS"),
            no_proxy: from_env("NO_PROXY").map(NoProxy::from).unwrap_or_default(),
        }
    }

    fn interceptor(&self) -> NoProxyInterceptor {
        NoProxyInterceptor(self.no_proxy.clone())
    }

    fn merge(mut self, other: &Self) -> Self {
        if !other.enabled {
            return self;
        }
        self.no_proxy.extend(other.no_proxy.clone());

        Self {
            enabled: self.enabled,
            http: self.http.or_else(|| other.http.clone()),
            https: self.https.or_else(|| other.https.clone()),
            no_proxy: self.no_proxy,
        }
    }

    pub fn build(&self, other: &Self) -> Self {
        // in order, we take first the environment variable,
        // the the global variables and then the service config
        Self::from_env().merge(&self).merge(other)
    }

    fn http_intercept(&self) -> Intercept {
        self.interceptor().intercept("http")
    }

    fn http_proxy(&self) -> Result<Option<Proxy>, InvalidUri> {
        if let Some(ref url) = self.http {
            Ok(Some(Proxy::new(self.http_intercept(), url.parse()?)))
        } else {
            Ok(None)
        }
    }

    fn https_intercept(&self) -> Intercept {
        self.interceptor().intercept("https")
    }

    fn https_proxy(&self) -> Result<Option<Proxy>, InvalidUri> {
        if let Some(ref url) = self.https {
            Ok(Some(Proxy::new(self.https_intercept(), url.parse()?)))
        } else {
            Ok(None)
        }
    }

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
    use super::*;

    #[test]
    fn merge() {
        let first = ProxyConfig {
            enabled: true,
            http: Some("http://1.2.3.4:5678".into()),
            https: None,
            no_proxy: NoProxy::from("127.0.0.1,google.com"),
        };
        let second = ProxyConfig {
            enabled: true,
            http: Some("http://1.2.3.5:5678".into()),
            https: Some("https://2.3.4.5:9876".into()),
            no_proxy: NoProxy::from("localhost"),
        };
        let result = first.merge(&second);
        assert_eq!(result.http, Some("http://1.2.3.4:5678".into()));
        assert_eq!(result.https, Some("https://2.3.4.5:9876".into()));
        assert!(result.no_proxy.matches(&"127.0.0.1".to_string()));
        assert!(result.no_proxy.matches(&"localhost".to_string()));
    }
}
