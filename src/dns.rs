use futures::{future::BoxFuture, FutureExt, TryFutureExt};
use futures01::Future;
use hyper::client::connect::dns::{Name, Resolve};
use hyper13::client::connect::dns::Name as Name13;
use snafu::ResultExt;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs},
    task::{Context, Poll},
};
use tokio::task::spawn_blocking;
use tower03::Service;

pub type ResolverFuture = Box<dyn Future<Item = LookupIp, Error = DnsError> + Send + 'static>;

pub struct LookupIp(std::vec::IntoIter<SocketAddr>);

#[derive(Debug, Clone, Copy)]
pub struct Resolver;

impl Resolver {
    pub fn lookup_ip_01(self, name: String) -> ResolverFuture {
        let fut = self.lookup_ip(name).boxed().compat();
        Box::new(fut)
    }

    pub async fn lookup_ip(self, name: String) -> Result<LookupIp, DnsError> {
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
            spawn_blocking(move || (name.as_ref(), dummy_port).to_socket_addrs())
                .await
                .context(JoinError)?
                .map(LookupIp)
                .context(UnableLookup)
        }
    }
}

impl Iterator for LookupIp {
    type Item = IpAddr;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|address| address.ip())
    }
}

impl Resolve for Resolver {
    type Addrs = LookupIp;
    type Future = Box<dyn Future<Item = LookupIp, Error = std::io::Error> + Send + 'static>;

    fn resolve(&self, name: Name) -> Self::Future {
        let fut = self
            .lookup_ip(name.as_str().to_owned())
            .boxed()
            .compat()
            .map_err(|e| {
                use std::io;
                io::Error::new(io::ErrorKind::Other, e)
            });
        Box::new(fut)
    }
}

impl Service<Name13> for Resolver {
    type Response = LookupIp;
    type Error = DnsError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, name: Name13) -> Self::Future {
        self.lookup_ip(name.as_str().to_owned()).boxed()
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
    use crate::test_util::runtime;

    fn resolve(name: &str) -> bool {
        let mut runtime = runtime();

        let resolver = Resolver;
        let fut = resolver.lookup_ip(name.to_owned());
        runtime.block_on_std(fut).is_ok()
    }

    #[test]
    fn resolve_vector() {
        assert!(resolve("vector.dev"));
    }

    #[test]
    fn resolve_localhost() {
        assert!(resolve("localhost"));
    }

    #[test]
    fn resolve_ipv4() {
        assert!(resolve("10.0.4.0"));
    }

    #[test]
    fn resolve_ipv6() {
        assert!(resolve("::1"));
    }
}
