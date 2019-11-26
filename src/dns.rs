use crate::topology::config::GlobalOptions;
use futures::{Async, Future, Poll};
use hyper::client::connect::dns::{Name, Resolve};
use snafu::Snafu;
use std::io;
use std::net::{AddrParseError, IpAddr, SocketAddr, ToSocketAddrs};
use std::vec::IntoIter;
use trust_dns_resolver::{
    config::{LookupIpStrategy, NameServerConfig, Protocol, ResolverConfig, ResolverOpts},
    error::ResolveErrorKind,
    AsyncResolver, BackgroundLookupIp,
};

/// Default port for DNS service.
const DNS_PORT: u16 = 53;

/// Default version will behave identically to std::net::ToSocketAddrs implementations.
#[derive(Clone, Default)]
pub struct DnsResolver {
    resolver: Option<AsyncResolver>,
}

impl DnsResolver {
    /// Returned future should be spinned up.
    pub fn new(
        config: &GlobalOptions,
    ) -> Result<(Self, Option<impl Future<Item = (), Error = ()>>), DnsError> {
        if config.dns_servers.is_empty() {
            return Ok((DnsResolver::default(), None));
        }

        let mut resolve_config = ResolverConfig::new();

        let mut errors = Vec::new();

        for s in config.dns_servers.iter() {
            let parsed = s
                .parse::<SocketAddr>()
                .or_else(|_| s.parse::<IpAddr>().map(|ip| SocketAddr::new(ip, DNS_PORT)));
            match parsed {
                Ok(socket_addr) => resolve_config.add_name_server(NameServerConfig {
                    socket_addr,
                    protocol: Protocol::Udp,
                    tls_dns_name: None,
                }),
                Err(error) => errors.push((s.clone(), error)),
            }
        }

        if !errors.is_empty() {
            return Err(DnsError::InputError { servers: errors });
        }

        let mut options = ResolverOpts::default();
        options.attempts = 2;
        options.validate = false;
        options.ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
        options.num_concurrent_reqs = 1;

        let (resolver, worker) = AsyncResolver::new(resolve_config, options);

        Ok((
            DnsResolver {
                resolver: Some(resolver),
            },
            Some(worker),
        ))
    }

    /// Resolves host:port address
    pub fn resolve_address(
        &self,
        s: &str,
    ) -> Result<impl Future<Item = IntoIter<SocketAddr>, Error = io::Error>, DnsError> {
        // Try to parse as a regular SocketAddr first
        let (future, port) = if let Some(address) = s.parse::<SocketAddr>().ok() {
            (ResolveFuture::IpAddr(address.ip()), address.port())
        } else {
            // 's' should contain name:port
            let mut parts = s.rsplitn(2, ':');
            let port = parts
                .next()
                .ok_or_else(|| DnsError::MissingPort {
                    address: s.to_owned(),
                })?
                .parse::<u16>()
                .map_err(|source| DnsError::InvalidPort {
                    address: s.to_owned(),
                    source,
                })?;
            let name = parts
                .next()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| DnsError::MissingHost {
                    address: s.to_owned(),
                })?
                .parse::<Name>()
                .map_err(|source| DnsError::InvalidHost {
                    address: s.to_owned(),
                    source,
                })?;

            (self.construct_future(name), port)
        };

        Ok(future.map(move |address| {
            address
                .map(|addr| (addr, port).into())
                .collect::<Vec<SocketAddr>>()
                .into_iter()
        }))
    }

    /// Resolves host
    pub fn resolve_host(&self, host: &str) -> Result<ResolveFuture, DnsError> {
        let name = host
            .parse::<Name>()
            .map_err(|error| DnsError::InvalidHost {
                address: host.to_owned(),
                source: error,
            })?;

        Ok(self.construct_future(name))
    }

    fn construct_future(&self, name: Name) -> ResolveFuture {
        // try to parse as a regular IpAddr first
        name.as_str()
            .parse::<IpAddr>()
            .ok()
            .map(ResolveFuture::IpAddr)
            // Else start resolution
            .unwrap_or_else(|| match self.resolver.as_ref() {
                Some(resolver) => ResolveFuture::Custom(resolver.lookup_ip(name.as_str()), name),
                None => ResolveFuture::System(name),
            })
    }
}

impl Resolve for DnsResolver {
    type Addrs = IntoIter<IpAddr>;
    /// A Future of the resolved set of addresses.
    type Future = ResolveFuture;
    /// Resolve a hostname.
    fn resolve(&self, name: Name) -> Self::Future {
        self.construct_future(name)
    }
}

pub enum ResolveFuture {
    /// Resolved
    IpAddr(IpAddr),
    /// Resolving using custom DNS servers
    Custom(BackgroundLookupIp, Name),
    /// Resolve using system resolver servers
    System(Name),
}

impl ResolveFuture {
    fn system_resolve(name: &Name) -> Poll<IntoIter<IpAddr>, io::Error> {
        let poll = tokio_threadpool::blocking(|| {
            (name.as_str(), 0).to_socket_addrs().map(|ips| {
                ips.map(|socket_addr| socket_addr.ip())
                    .collect::<Vec<_>>()
                    .into_iter()
            })
        });
        match poll {
            Poll::Ok(Async::NotReady) => Poll::Ok(Async::NotReady),
            Poll::Ok(Async::Ready(Ok(ips))) => Poll::Ok(Async::Ready(ips)),
            Poll::Ok(Async::Ready(Err(error))) => Poll::Err(error),
            Poll::Err(error) => Poll::Err(io::Error::new(io::ErrorKind::Other, error)),
        }
    }
}

impl Future for ResolveFuture {
    type Item = IntoIter<IpAddr>;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self {
            ResolveFuture::IpAddr(addr) => Poll::Ok(Async::Ready(vec![addr.clone()].into_iter())),
            ResolveFuture::Custom(future, name) => match future.poll() {
                Poll::Ok(Async::NotReady) => Poll::Ok(Async::NotReady),
                Poll::Ok(Async::Ready(ips)) => {
                    Poll::Ok(Async::Ready(ips.iter().collect::<Vec<_>>().into_iter()))
                }
                Poll::Err(error) => {
                    match error.kind() {
                        ResolveErrorKind::NoRecordsFound { .. } => debug!("No records found"),
                        _ => {
                            error!(message = "Error while resolving Domain Name", error = ?error);
                        }
                    }
                    ResolveFuture::system_resolve(name)
                }
            },
            ResolveFuture::System(name) => ResolveFuture::system_resolve(name),
        }
    }
}

#[derive(Debug, Snafu)]
pub enum DnsError {
    #[snafu(display("Invalid DNS server IPs: {:?}", servers.iter().map(|(s,e)| format!("({:?} : {})",s,e)).collect::<Vec<_>>()))]
    InputError {
        servers: Vec<(String, AddrParseError)>,
    },
    #[snafu(display("Missing port in address {:?}", address))]
    MissingPort { address: String },
    #[snafu(display("Missing host in address {:?}", address))]
    MissingHost { address: String },
    #[snafu(display("Invalid port in address {:?}, source: {}", address, source))]
    InvalidPort {
        address: String,
        source: std::num::ParseIntError,
    },
    #[snafu(display("Invalid host in {:?}, source: {}", address, source))]
    InvalidHost {
        address: String,
        source: hyper::client::connect::dns::InvalidNameError,
    },
    #[snafu(display("{}", source))]
    IO { source: io::Error },
}

impl From<io::Error> for DnsError {
    fn from(error: io::Error) -> Self {
        DnsError::IO { source: error }
    }
}

#[cfg(test)]
mod tests {
    use super::DnsResolver;
    use crate::runtime::Runtime;
    use crate::test_util::{next_addr, runtime};
    use crate::topology::config::GlobalOptions;
    use futures::Future;
    use std::collections::BTreeMap;
    use std::net::{IpAddr, SocketAddr, UdpSocket};
    use std::str::FromStr;
    use tokio::prelude::{future::poll_fn, Async};
    use trust_dns::rr::{record_data::RData, LowerName, Name, RecordSet, RecordType, RrKey};
    use trust_dns_proto::rr::rdata::soa::SOA;
    use trust_dns_server::{
        authority::{Catalog, ZoneType},
        store::in_memory::InMemoryAuthority,
        ServerFuture,
    };

    fn join(subdomain: &str, domain: &str) -> String {
        subdomain.to_owned() + "." + domain
    }

    fn lower_name(name: &str) -> LowerName {
        LowerName::new(&Name::from_str(name).unwrap())
    }

    /// subdomain.domain
    fn dns_authority(subdomains: &[(&str, IpAddr)], domain: &str) -> Catalog {
        let mut map = BTreeMap::new();

        // SOA record
        let key = RrKey::new(lower_name(domain), RecordType::SOA);
        let mut records = RecordSet::new(&Name::from_str(domain).unwrap(), RecordType::SOA, 0);
        records.new_record(&RData::SOA(SOA::new(
            Name::from_str(domain).unwrap(),
            Name::from_str(join("admin\\", domain).as_str()).unwrap(),
            0,
            3600,
            1800,
            604800,
            86400,
        )));
        map.insert(key, records);

        for &(subdomain, ip) in subdomains.iter() {
            match ip {
                IpAddr::V4(ip) => {
                    // A record
                    let key =
                        RrKey::new(lower_name(join(subdomain, domain).as_str()), RecordType::A);
                    let mut records = RecordSet::new(
                        &Name::from_str(join(subdomain, domain).as_str()).unwrap(),
                        RecordType::A,
                        0,
                    );
                    records.new_record(&RData::A(ip));
                    map.insert(key, records);
                }
                IpAddr::V6(ip) => {
                    // AAAA record
                    let key = RrKey::new(
                        lower_name(join(subdomain, domain).as_str()),
                        RecordType::AAAA,
                    );
                    let mut records = RecordSet::new(
                        &Name::from_str(join(subdomain, domain).as_str()).unwrap(),
                        RecordType::AAAA,
                        0,
                    );
                    records.new_record(&RData::AAAA(ip));
                    map.insert(key, records);
                }
            }
        }

        let authority = InMemoryAuthority::new(
            Name::from_str(domain).unwrap(),
            map,
            ZoneType::Master,
            false,
        )
        .unwrap();
        let mut handler = Catalog::new();
        handler.upsert(lower_name(domain), Box::new(authority));

        handler
    }

    /// subdomain.domain
    fn dns_server(
        subdomains: &[(&'static str, IpAddr)],
        domain: &'static str,
        rt: &mut Runtime,
    ) -> SocketAddr {
        let subdomains = subdomains.iter().map(|&v| v).collect::<Vec<_>>();
        let address = next_addr();

        // Start DNS server
        rt.spawn(poll_fn(move || {
            let handler = dns_authority(subdomains.as_slice(), domain);
            let server = ServerFuture::new(handler);
            let socket = UdpSocket::bind(address).unwrap();
            server.register_socket_std(socket);
            debug!("DNS started at: {}", address);
            Ok(Async::Ready(()))
        }));

        // Wait for DNS server to start
        std::thread::sleep(std::time::Duration::from_secs(1));

        address
    }

    fn resolver_for(
        subdomains: &[(&'static str, IpAddr)],
        domain: &'static str,
        rt: &mut Runtime,
    ) -> DnsResolver {
        let server = dns_server(subdomains, domain, rt);
        let mut config = GlobalOptions::default();
        config.dns_servers = vec![format!("{}", server)];

        let (dns, worker) = DnsResolver::new(&config).unwrap();
        worker.map(|worker| rt.spawn(worker));
        dns
    }

    fn future_ready<F: Future>(mut future: F) -> F::Item
    where
        F::Error: std::fmt::Debug,
    {
        match future.poll() {
            Ok(Async::Ready(item)) => item,
            Ok(Async::NotReady) => panic!("Future was not ready"),
            Err(error) => panic!("Future was not ready, but errored: {:?}", error),
        }
    }

    #[test]
    fn resolve_host() {
        let mut runtime = runtime();
        let domain = "vector.test";

        let target = "10.45.12.34".parse().unwrap();
        let resolver = resolver_for(&[("name", target)], domain, &mut runtime);

        assert_eq!(
            target,
            runtime
                .block_on(
                    resolver
                        .resolve_host(join("name", domain).as_str())
                        .unwrap(),
                )
                .unwrap()
                .next()
                .unwrap()
        );
    }

    #[test]
    fn resolve_address() {
        let mut runtime = runtime();
        let domain = "vector.test";

        let target = "10.45.12.35".parse().unwrap();
        let resolver = resolver_for(&[("address", target)], domain, &mut runtime);

        assert_eq!(
            SocketAddr::from((target, 9000)),
            runtime
                .block_on(
                    resolver
                        .resolve_address((join("address", domain) + ":9000").as_str())
                        .unwrap(),
                )
                .unwrap()
                .next()
                .unwrap()
        );
    }

    #[test]
    fn multiple_dns_servers() {
        let mut runtime = Runtime::with_thread_count(3).unwrap();
        let domain_a = "meta.vec";
        let domain_b = "metab.fvec";
        let domain_c = "vectorc.test";

        let target_a = "10.45.12.34".parse().unwrap();
        let target_b = "10.45.13.34".parse().unwrap();
        let target_c = "10.45.14.35".parse().unwrap();
        let server0 = dns_server(&[("a", target_a)], domain_a, &mut runtime);
        let server1 = dns_server(&[("b", target_b)], domain_b, &mut runtime);
        let server2 = dns_server(&[("c", target_c)], domain_c, &mut runtime);
        let mut config = GlobalOptions::default();
        config.dns_servers = vec![
            format!("{}", server0),
            format!("{}", server1),
            format!("{}", server2),
        ];

        let (resolver, worker) = DnsResolver::new(&config).unwrap();
        worker.map(|worker| runtime.spawn(worker));

        assert_eq!(
            target_a,
            runtime
                .block_on(resolver.resolve_host(join("a", domain_a).as_str()).unwrap())
                .unwrap()
                .next()
                .unwrap()
        );

        assert_eq!(
            target_b,
            runtime
                .block_on(resolver.resolve_host(join("b", domain_b).as_str()).unwrap())
                .unwrap()
                .next()
                .unwrap()
        );

        assert_eq!(
            target_c,
            runtime
                .block_on(resolver.resolve_host(join("c", domain_c).as_str()).unwrap())
                .unwrap()
                .next()
                .unwrap()
        );
    }

    #[test]
    fn resolve_address_missing_port() {
        assert!(DnsResolver::default()
            .resolve_address("vector.test")
            .is_err());
    }

    #[test]
    fn resolve_address_missing_host() {
        assert!(DnsResolver::default().resolve_address(":9900").is_err());
    }

    #[test]
    fn resolve_address_socket_v4() {
        assert_eq!(
            future_ready(
                DnsResolver::default()
                    .resolve_address("89.03.02.84:9900")
                    .unwrap()
            )
            .next()
            .unwrap(),
            SocketAddr::from_str("89.03.02.84:9900").unwrap()
        );
    }

    #[test]
    fn resolve_address_socket_v6() {
        assert_eq!(
            future_ready(
                DnsResolver::default()
                    .resolve_address("[::0]:9900")
                    .unwrap()
            )
            .next()
            .unwrap(),
            SocketAddr::from_str("[::0]:9900").unwrap()
        );
    }

    #[test]
    fn resolve_host_v4() {
        assert_eq!(
            future_ready(DnsResolver::default().resolve_host("89.03.02.84").unwrap())
                .next()
                .unwrap(),
            IpAddr::from_str("89.03.02.84").unwrap()
        );
    }
}
