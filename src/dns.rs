#![allow(missing_docs)]
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs},
    task::{Context, Poll},
};

use futures::{FutureExt, future::BoxFuture};
use hyper::client::connect::dns::Name;
use rand::Rng;
use snafu::ResultExt;
use tokio::task::spawn_blocking;
use tower::Service;
use vector_lib::configurable::configurable_component;

/// Controls how resolved DNS addresses are selected when multiple addresses are returned.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DnsAddressSelection {
    /// Use the first address returned by the system resolver (default).
    #[default]
    First,
    /// Pick a single random address from the resolved set.
    Random,
}

pub struct LookupIp(std::vec::IntoIter<SocketAddr>);

/// The standard Vector DNS resolver.
///
/// Resolves hostnames via the system's `getaddrinfo` and applies the configured
/// [`DnsAddressSelection`] policy to the returned addresses.
#[derive(Debug, Clone, Copy)]
pub struct Resolver {
    pub selection: DnsAddressSelection,
}

impl Default for Resolver {
    fn default() -> Self {
        Self {
            selection: DnsAddressSelection::First,
        }
    }
}

impl Resolver {
    pub(crate) async fn lookup_ip(self, name: String) -> Result<LookupIp, DnsError> {
        // We need to add port with the name so that `to_socket_addrs`
        // resolves it properly. We will be discarding the port afterwards.
        //
        // Any port will do, but `9` is a well defined port for discarding
        // packets.
        let dummy_port = 9;
        let selection = self.selection;
        // https://tools.ietf.org/html/rfc6761#section-6.3
        if name == "localhost" {
            // Not all operating systems support `localhost` as IPv6 `::1`, so
            // we resolving it to it's IPv4 value.
            Ok(LookupIp(
                vec![SocketAddr::new(Ipv4Addr::LOCALHOST.into(), dummy_port)].into_iter(),
            ))
        } else {
            let addrs = spawn_blocking(move || {
                // strip IPv6 prefix and suffix
                let name_str = name.as_str();
                let name_ref = name_str
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .unwrap_or(name_str);
                (name_ref, dummy_port).to_socket_addrs()
            })
            .await
            .context(JoinSnafu)?
            .context(UnableLookupSnafu)?;

            let mut addrs: Vec<SocketAddr> = addrs.collect();
            apply_selection(&mut addrs, selection);
            Ok(LookupIp(addrs.into_iter()))
        }
    }
}

fn apply_selection(addrs: &mut Vec<SocketAddr>, selection: DnsAddressSelection) {
    match selection {
        DnsAddressSelection::First => {}
        DnsAddressSelection::Random => {
            if !addrs.is_empty() {
                let idx = rand::rng().random_range(0..addrs.len());
                let chosen = addrs[idx];
                addrs.clear();
                addrs.push(chosen);
            }
        }
    }
}

impl Iterator for LookupIp {
    type Item = IpAddr;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|address| address.ip())
    }
}

impl Service<Name> for Resolver {
    type Response = LookupIp;
    type Error = DnsError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, name: Name) -> Self::Future {
        self.lookup_ip(name.as_str().to_owned()).boxed()
    }
}

/// A resolver compatible with hyper's `HttpConnector` that yields `SocketAddr`.
///
/// Unlike [`Resolver`] (which yields `IpAddr`), this resolver returns full socket
/// addresses and applies the configured [`DnsAddressSelection`] policy. It is designed
/// to be used with `HttpConnector::new_with_resolver()`.
#[derive(Debug, Clone, Copy)]
pub struct HyperResolver {
    pub selection: DnsAddressSelection,
}

/// Iterator over resolved socket addresses for [`HyperResolver`].
pub struct SocketAddrIter(std::vec::IntoIter<SocketAddr>);

impl Iterator for SocketAddrIter {
    type Item = SocketAddr;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl Service<Name> for HyperResolver {
    type Response = SocketAddrIter;
    type Error = DnsError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, name: Name) -> Self::Future {
        let selection = self.selection;
        async move {
            let dummy_port = 0;
            let name_string = name.as_str().to_owned();

            if name_string == "localhost" {
                return Ok(SocketAddrIter(
                    vec![SocketAddr::new(Ipv4Addr::LOCALHOST.into(), dummy_port)].into_iter(),
                ));
            }

            let addrs = spawn_blocking(move || {
                let name_str = name_string.as_str();
                let name_ref = name_str
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .unwrap_or(name_str);
                (name_ref, dummy_port).to_socket_addrs()
            })
            .await
            .context(JoinSnafu)?
            .context(UnableLookupSnafu)?;

            let mut addrs: Vec<SocketAddr> = addrs.collect();
            apply_selection(&mut addrs, selection);
            Ok(SocketAddrIter(addrs.into_iter()))
        }
        .boxed()
    }
}

#[derive(Debug, snafu::Snafu)]
pub enum DnsError {
    #[snafu(display("Unable to resolve name: {}", source))]
    UnableLookup { source: tokio::io::Error },
    #[snafu(display("Failed to join with resolving future: {}", source))]
    JoinError { source: tokio::task::JoinError },
}

#[cfg(test)]
mod tests {
    use super::Resolver;

    async fn resolve(name: &str) -> bool {
        let resolver = Resolver::default();
        resolver.lookup_ip(name.to_owned()).await.is_ok()
    }

    #[tokio::test]
    async fn resolve_example() {
        assert!(resolve("example.com").await);
    }

    #[tokio::test]
    async fn resolve_localhost() {
        assert!(resolve("localhost").await);
    }

    #[tokio::test]
    async fn resolve_ipv4() {
        assert!(resolve("10.0.4.0").await);
    }

    #[tokio::test]
    async fn resolve_ipv6() {
        assert!(resolve("::1").await);
    }
}
