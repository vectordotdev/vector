use crate::runtime::TaskExecutor;
use futures::{compat::Future01CompatExt, future::BoxFuture, FutureExt};
use futures01::{future, Future};
use hyper::client::connect::dns::{Name, Resolve};
use hyper13::client::connect::dns::Name as Name13;
use snafu::{futures01::FutureExt as _, ResultExt};
use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    str::FromStr,
    task::{Context, Poll},
};
use tower03::Service;
use trust_dns_resolver::{
    config::{LookupIpStrategy, NameServerConfig, Protocol, ResolverConfig, ResolverOpts},
    lookup_ip::LookupIpIntoIter,
    system_conf, AsyncResolver,
};

/// Default port for DNS service.
const DNS_PORT: u16 = 53;

pub type ResolverFuture = Box<dyn Future<Item = LookupIp, Error = DnsError> + Send + 'static>;

#[derive(Debug, Clone)]
pub struct Resolver {
    inner: AsyncResolver,
}

pub enum LookupIp {
    Single(Option<IpAddr>),
    Query(LookupIpIntoIter),
}

impl Resolver {
    pub fn new(dns_servers: Vec<String>, exec: TaskExecutor) -> Result<Self, DnsError> {
        let (config, opt) = if !dns_servers.is_empty() {
            let mut config = ResolverConfig::new();

            let mut errors = Vec::new();

            for s in dns_servers.iter() {
                let parsed = s
                    .parse::<SocketAddr>()
                    .or_else(|_| s.parse::<IpAddr>().map(|ip| SocketAddr::new(ip, DNS_PORT)));

                match parsed {
                    Ok(socket_addr) => config.add_name_server(NameServerConfig {
                        socket_addr,
                        protocol: Protocol::Udp,
                        tls_dns_name: None,
                    }),
                    Err(error) => errors.push(format!(
                        "Unable to parse dns server: {}, because {}",
                        s, error
                    )),
                }
            }

            if !errors.is_empty() {
                return Err(DnsError::ServerList { errors });
            }

            let mut opts = ResolverOpts::default();
            opts.attempts = 2;
            opts.validate = false;
            opts.ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
            // FIXME: multipe requests fails when this is commented out
            opts.num_concurrent_reqs = 1;

            (config, opts)
        } else {
            #[cfg(feature = "disable-resolv-conf")]
            let res = (Default::default(), Default::default());
            #[cfg(not(feature = "disable-resolv-conf"))]
            let res = system_conf::read_system_conf().context(ReadSystemConf)?;
            res
        };

        let (inner, bg_task) = AsyncResolver::new(config, opt);

        exec.spawn(bg_task);

        Ok(Self { inner })
    }

    pub fn lookup_ip(&self, name: impl AsRef<str>) -> ResolverFuture {
        if let Ok(ip) = IpAddr::from_str(name.as_ref()) {
            return Box::new(future::ok(LookupIp::Single(Some(ip))));
        }

        Box::new(
            self.inner
                .lookup_ip(name.as_ref())
                .context(UnableLookup)
                .map(|lu| LookupIp::Query(lu.into_iter())),
        )
    }
}

impl Iterator for LookupIp {
    type Item = IpAddr;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            LookupIp::Single(ip) => ip.take(),
            LookupIp::Query(iter) => iter.next(),
        }
    }
}

impl Resolve for Resolver {
    type Addrs = LookupIp;
    type Future = Box<dyn Future<Item = Self::Addrs, Error = std::io::Error> + Send + 'static>;

    fn resolve(&self, name: Name) -> Self::Future {
        let fut = self.lookup_ip(name.as_str()).map_err(|e| {
            use std::io;
            io::Error::new(io::ErrorKind::Other, e)
        });
        Box::new(fut)
    }
}

impl Service<Name13> for Resolver {
    type Response = LookupIp;
    type Error = std::io::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, name: Name13) -> Self::Future {
        self.lookup_ip(name.as_str())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            .compat()
            .boxed()
    }
}

#[derive(Debug, snafu::Snafu)]
pub enum DnsError {
    #[snafu(display("Unable to parse dns servers: {}", errors.join(", ")))]
    ServerList { errors: Vec<String> },
    #[cfg(windows)]
    #[snafu(display("Unable to read system dns config: {}", source))]
    ReadSystemConf {
        #[snafu(source(from(trust_dns_resolver::error::ResolveError, ResolveError::from)))]
        source: ResolveError,
    },
    #[cfg(unix)]
    #[snafu(display("Unable to read system dns config: {}", source))]
    ReadSystemConf { source: std::io::Error },
    #[snafu(display("Unable to resolve name: {}", source))]
    UnableLookup {
        #[snafu(source(from(trust_dns_resolver::error::ResolveError, ResolveError::from)))]
        source: ResolveError,
    },
    #[snafu(display("Invalid dns name: {}", source))]
    InvalidName {
        #[snafu(source(from(trust_dns_proto::error::ProtoError, ProtoError::from)))]
        source: ProtoError,
    },
}

// TODO: Upstream this change, we require this newtype to impl `std::error::Error`.
#[derive(Debug)]
pub struct ResolveError(trust_dns_resolver::error::ResolveError);

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ResolveError {}

impl From<trust_dns_resolver::error::ResolveError> for ResolveError {
    fn from(t: trust_dns_resolver::error::ResolveError) -> Self {
        ResolveError(t)
    }
}

// TODO: Upstream this change, we require this newtype to impl `std::error::Error`.
#[derive(Debug)]
pub struct ProtoError(trust_dns_proto::error::ProtoError);

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ProtoError {}

impl From<trust_dns_proto::error::ProtoError> for ProtoError {
    fn from(t: trust_dns_proto::error::ProtoError) -> Self {
        ProtoError(t)
    }
}

#[cfg(test)]
mod tests {
    use super::Resolver;
    use crate::runtime::Runtime;
    use crate::test_util::{next_addr, runtime};
    use crate::topology::config::GlobalOptions;
    use std::collections::BTreeMap;
    use std::net::{IpAddr, SocketAddr, UdpSocket};
    use std::str::FromStr;
    use tokio01::prelude::{future::poll_fn, Async};
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
    ) -> Resolver {
        let server = dns_server(subdomains, domain, rt);
        let mut config = GlobalOptions::default();
        config.dns_servers = vec![format!("{}", server)];

        Resolver::new(config.dns_servers.clone(), rt.executor()).unwrap()
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
                .block_on(resolver.lookup_ip(join("name", domain).as_str()))
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

        let resolver = Resolver::new(config.dns_servers.clone(), runtime.executor()).unwrap();

        assert_eq!(
            target_a,
            runtime
                .block_on(resolver.lookup_ip(join("a", domain_a).as_str()))
                .unwrap()
                .next()
                .unwrap()
        );

        assert_eq!(
            target_b,
            runtime
                .block_on(resolver.lookup_ip(join("b", domain_b).as_str()))
                .unwrap()
                .next()
                .unwrap()
        );

        assert_eq!(
            target_c,
            runtime
                .block_on(resolver.lookup_ip(join("c", domain_c).as_str()))
                .unwrap()
                .next()
                .unwrap()
        );
    }

    #[test]
    fn resolve_ipv4() {
        let mut rt = runtime();
        let resolver = Resolver::new(Vec::new(), rt.executor()).unwrap();

        let mut res = rt.block_on(resolver.lookup_ip("127.0.0.1")).unwrap();

        assert_eq!(res.next(), Some(IpAddr::from_str("127.0.0.1").unwrap()));
    }

    #[test]
    fn resolve_ipv6() {
        let mut rt = runtime();
        let resolver = Resolver::new(Vec::new(), rt.executor()).unwrap();

        let mut res = rt.block_on(resolver.lookup_ip("::0")).unwrap();

        assert_eq!(res.next(), Some(IpAddr::from_str("::0").unwrap()));

        let mut res = rt
            .block_on(resolver.lookup_ip("2001:0db8:85a3:0000:0000:8a2e:0370:7334"))
            .unwrap();

        assert_eq!(
            res.next(),
            Some(IpAddr::from_str("2001:0db8:85a3:0000:0000:8a2e:0370:7334").unwrap())
        );
    }
}
