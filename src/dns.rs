#![allow(missing_docs)]
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};

use snafu::ResultExt;
use tokio::task::spawn_blocking;

pub struct LookupIp(std::vec::IntoIter<SocketAddr>);

#[derive(Debug, Clone, Copy)]
pub(super) struct Resolver;

impl Resolver {
    pub(crate) async fn lookup_ip(self, name: String) -> Result<LookupIp, DnsError> {
        // We need to add port with the name so that `to_socket_addrs`
        // resolves it properly. We will be discarding the port afterwards.
        //
        // Any port will do, but `9` is a well defined port for discarding
        // packets.
        let dummy_port = 9;
        // https://tools.ietf.org/html/rfc6761#section-6.3
        if name == "localhost" {
            // Not all operating systems support `localhost` as IPv6 `::1`, so
            // we resolving it to it's IPv4 value.
            Ok(LookupIp(
                vec![SocketAddr::new(Ipv4Addr::LOCALHOST.into(), dummy_port)].into_iter(),
            ))
        } else {
            spawn_blocking(move || {
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
            .map(LookupIp)
            .context(UnableLookupSnafu)
        }
    }
}

impl Iterator for LookupIp {
    type Item = IpAddr;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|address| address.ip())
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
        let resolver = Resolver;
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
