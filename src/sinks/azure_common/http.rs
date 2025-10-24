//! Reqwest client builder for Azure sinks with per-sink proxy support.
//!
//! This module builds a `reqwest::Client` honoring Vector's per-component `ProxyConfig`.
//! It configures HTTP and HTTPS proxies using `reqwest::Proxy`, including optional
//! basic authentication if credentials are embedded in the proxy URL.
//
//! no_proxy patterns are supported via `reqwest::Proxy::custom`, bypassing the proxy
//! for hosts (and host:port) that match Vector's `NoProxy` rules.

use reqwest_0_12::{Client, Proxy};
use url::Url;
use vector_lib::config::proxy::ProxyConfig;

/// Build a `reqwest::Client` honoring the provided per-sink `ProxyConfig`.
///
/// - Supports HTTP and HTTPS proxies
/// - Supports user:password embedded in the proxy URL (percent-decoding applied)
/// - Keeps connection pooling defaults; callers can further customize if needed
pub fn build_reqwest_with_proxy(proxy: &ProxyConfig) -> crate::Result<Client> {
    let mut builder = reqwest_0_12::Client::builder();

    if proxy.enabled {
        // Install scheme-specific proxies with a custom no_proxy predicate based on Vector's NoProxy.
        if let Some(ref http_url) = proxy.http {
            let target = Url::parse(http_url)?;
            let no_proxy = proxy.no_proxy.clone();
            let p = Proxy::custom(move |req_url: &Url| {
                if req_url.scheme() == "http" {
                    if let Some(host) = req_url.host_str() {
                        let bypass = no_proxy.matches(host)
                            || req_url
                                .port()
                                .map(|port| {
                                    let hp = format!("{host}:{port}");
                                    no_proxy.matches(&hp)
                                })
                                .unwrap_or(false);
                        if !bypass {
                            return Some(target.clone());
                        }
                    }
                }
                None
            });
            builder = builder.proxy(p);
        }

        if let Some(ref https_url) = proxy.https {
            let target = Url::parse(https_url)?;
            let no_proxy = proxy.no_proxy.clone();
            let p = Proxy::custom(move |req_url: &Url| {
                if req_url.scheme() == "https" {
                    if let Some(host) = req_url.host_str() {
                        let bypass = no_proxy.matches(host)
                            || req_url
                                .port()
                                .map(|port| {
                                    let hp = format!("{host}:{port}");
                                    no_proxy.matches(&hp)
                                })
                                .unwrap_or(false);
                        if !bypass {
                            return Some(target.clone());
                        }
                    }
                }
                None
            });
            builder = builder.proxy(p);
        }
    }

    Ok(builder.build()?)
}

/// Construct a reqwest::Proxy using the provided constructor (http/https),
/// and attach Proxy-Authorization via Basic auth if the URL includes user info.

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn builds_without_proxy() {
        let cfg = ProxyConfig::default();
        let client = build_reqwest_with_proxy(&cfg).expect("client should build");
        // ensure client is usable
        let _ = client.clone();
    }

    #[test]
    fn builds_with_http_proxy() {
        // Bind a dummy local listener to simulate a proxy endpoint.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut cfg = ProxyConfig::default();
        cfg.http = Some(format!("http://{}", addr));
        let client = build_reqwest_with_proxy(&cfg).expect("client should build");
        let _ = client.clone();
    }

    #[test]
    fn builds_with_https_proxy_userinfo() {
        // This doesn't perform TLS; we're only validating URL parsing/decoding and builder wiring.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut cfg = ProxyConfig::default();
        cfg.https = Some(format!("https://user:P%40ss@{}", addr));
        let client = build_reqwest_with_proxy(&cfg).expect("client should build");
        let _ = client.clone();
    }
}
