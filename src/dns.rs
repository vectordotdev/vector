use crate::runtime::Runtime;
use crate::topology::config::GlobalOptions;
use futures::{Async, Future, Poll};
use hyper::client::connect::dns::{Name, Resolve};
use std::error;
use std::fmt;
use std::io;
use std::net::AddrParseError;
use std::net::ToSocketAddrs;
use std::net::*;
use std::vec::IntoIter;
use trust_dns_resolver::config::*;
use trust_dns_resolver::error::ResolveErrorKind;
use trust_dns_resolver::AsyncResolver;
use trust_dns_resolver::BackgroundLookupIp;

/// Default port for DNS service.
const DNS_PORT: u16 = 53;

/// Default version will behave identically to std::net::ToSocketAddrs implementations.
#[derive(Clone, Default)]
pub struct DnsResolver {
    resolver: Option<AsyncResolver>,
}

impl DnsResolver {
    pub fn new<'a, R: Into<Option<&'a mut Runtime>>>(
        config: &GlobalOptions,
        runtime: R,
    ) -> Result<Self, DnsError> {
        if config.dns.is_empty() {
            return Ok(DnsResolver::default());
        }

        let mut resolve_config = ResolverConfig::new();

        let mut errors = Vec::new();

        for s in config.dns.iter() {
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

        if let Some(runtime) = runtime.into() {
            runtime.spawn(worker);
        } else {
            // spin up worker thread for worker future
            std::thread::spawn(move || {
                // Worker futer will finish when all DnsResolvers are dropped.
                // Then this thread will also end.
                tokio::runtime::current_thread::run(worker);
            });
        }

        Ok(DnsResolver {
            resolver: Some(resolver),
        })
    }

    /// Resolves host:port address
    /// Should be only called from outside tokio. From inside tokio, Async version should be used.
    pub fn resolve_address(&self, s: &str) -> Result<IntoIter<SocketAddr>, DnsError> {
        // Try to parse as a regular SocketAddr first
        if let Some(addr) = s.parse().ok() {
            return Ok(vec![addr].into_iter());
        }

        // Now 's' should contain name:port
        let mut parts = s.rsplitn(2, ':');
        let port = parts
            .next()
            .ok_or_else(|| DnsError::MissingPort(s.to_owned()))?
            .parse::<u16>()
            .map_err(|error| DnsError::InvalidPort(s.to_owned(), error))?;
        let name = parts
            .next()
            .ok_or_else(|| DnsError::MissingHost(s.to_owned()))?
            .parse::<Name>()
            .map_err(|error| DnsError::InvalidHost(s.to_owned(), error))?;

        let addresses = self.resolve_name(name)?;

        Ok(addresses
            .map(|addr| (addr, port).into())
            .collect::<Vec<SocketAddr>>()
            .into_iter())
    }

    /// Resolves host
    /// Should be only called from outside tokio. From inside tokio, Async version should be used.
    pub fn resolve_host(&self, host: &str) -> Result<IntoIter<IpAddr>, DnsError> {
        let name = host
            .parse::<Name>()
            .map_err(|error| DnsError::InvalidHost(host.to_owned(), error))?;

        self.resolve_name(name).map_err(Into::into)
    }

    /// Should be only called from outside tokio. From inside tokio, Async version should be used.
    fn resolve_name(&self, name: Name) -> Result<IntoIter<IpAddr>, io::Error> {
        let future = self.construct_future(name);
        // Will panic if called from inside tokio.
        tokio::runtime::current_thread::block_on_all(future)
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
        match (name.as_str(), 0).to_socket_addrs() {
            Ok(ips) => Poll::Ok(Async::Ready(
                ips.map(|socket_addr| socket_addr.ip())
                    .collect::<Vec<_>>()
                    .into_iter(),
            )),
            Err(error) => Poll::Err(error),
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

#[derive(Debug)]
pub enum DnsError {
    InputError {
        servers: Vec<(String, AddrParseError)>,
    },
    MissingPort(String),
    MissingHost(String),
    InvalidPort(String, std::num::ParseIntError),
    InvalidHost(String, hyper::client::connect::dns::InvalidNameError),
    IO(io::Error),
}

impl error::Error for DnsError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            DnsError::InvalidPort(_, error) => Some(error),
            DnsError::InvalidHost(_, error) => Some(error),
            DnsError::IO(error) => Some(error),
            _ => None,
        }
    }
}

impl fmt::Display for DnsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DnsError::InputError { servers } => {
                writeln!(f, "Invalid DNS server IPs:")?;
                for (s, e) in servers {
                    writeln!(f, " - {:?} {}", s, e)?;
                }
                Ok(())
            }
            DnsError::MissingPort(input) => writeln!(f, "Missing port in address {:?}", input),
            DnsError::MissingHost(input) => writeln!(f, "Missing host in address {:?}", input),
            DnsError::InvalidPort(input, error) => {
                writeln!(f, "Invalid port in address {:?}, reason: {}", input, error)
            }
            DnsError::InvalidHost(input, error) => {
                writeln!(f, "Invalid host in {:?}, reason: {}", input, error)
            }
            DnsError::IO(error) => writeln!(f, "{}", error),
        }
    }
}

impl From<io::Error> for DnsError {
    fn from(error: io::Error) -> Self {
        DnsError::IO(error)
    }
}

#[cfg(test)]
mod tests {
    use super::DnsResolver;
    use crate::runtime::Runtime;
    use crate::test_util::{next_addr, runtime};
    use crate::topology::config::GlobalOptions;
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
        config.dns = vec![format!("{}", server)];

        DnsResolver::new(&config, rt).unwrap()
    }

    #[test]
    fn resolve_host() {
        let mut runtime = runtime();
        let domain = "vector.test";

        let target = "10.45.12.34".parse().unwrap();
        let resolver = resolver_for(&[("name", target)], domain, &mut runtime);

        assert_eq!(
            target,
            resolver
                .resolve_host(join("name", domain).as_str())
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
            resolver
                .resolve_address((join("address", domain) + ":9000").as_str())
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
        config.dns = vec![
            format!("{}", server0),
            format!("{}", server1),
            format!("{}", server2),
        ];

        let resolver = DnsResolver::new(&config, &mut runtime).unwrap();

        assert_eq!(
            target_a,
            resolver
                .resolve_host(join("a", domain_a).as_str())
                .unwrap()
                .next()
                .unwrap()
        );

        assert_eq!(
            target_b,
            resolver
                .resolve_host(join("b", domain_b).as_str())
                .unwrap()
                .next()
                .unwrap()
        );

        assert_eq!(
            target_c,
            resolver
                .resolve_host(join("c", domain_c).as_str())
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
            DnsResolver::default()
                .resolve_address("89.03.02.84:9900")
                .unwrap()
                .next()
                .unwrap(),
            SocketAddr::from_str("89.03.02.84:9900").unwrap()
        );
    }

    #[test]
    fn resolve_address_socket_v6() {
        assert_eq!(
            DnsResolver::default()
                .resolve_address("[::0]:9900")
                .unwrap()
                .next()
                .unwrap(),
            SocketAddr::from_str("[::0]:9900").unwrap()
        );
    }

    #[test]
    fn resolve_host_v4() {
        assert_eq!(
            DnsResolver::default()
                .resolve_host("89.03.02.84")
                .unwrap()
                .next()
                .unwrap(),
            IpAddr::from_str("89.03.02.84").unwrap()
        );
    }
}
