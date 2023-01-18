#[cfg(feature = "sources-utils-net-tcp")]
mod tcp;
#[cfg(feature = "sources-utils-net-udp")]
mod udp;

use std::{fmt, net::SocketAddr};

use serde::{de, Deserialize, Deserializer};
use vector_config::configurable_component;

use crate::config::{Protocol, Resource};

#[cfg(feature = "sources-utils-net-tcp")]
pub use self::tcp::{TcpNullAcker, TcpSource, TcpSourceAck, TcpSourceAcker};
#[cfg(feature = "sources-utils-net-udp")]
pub use self::udp::try_bind_udp_socket;

/// A listening address that can be given directly or be managed via `systemd` socket activation.
#[configurable_component]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum SocketListenAddr {
    /// An IPv4/IPv6 address and port.
    SocketAddr(SocketAddr),

    /// A file descriptor identifier that is given from, and managed by, the socket activation feature of `systemd`.
    #[serde(deserialize_with = "parse_systemd_fd")]
    SystemdFd(usize),
}

impl SocketListenAddr {
    const fn as_resource(self, protocol: Protocol) -> Resource {
        match self {
            Self::SocketAddr(addr) => match protocol {
                Protocol::Tcp => Resource::tcp(addr),
                Protocol::Udp => Resource::udp(addr),
            },
            Self::SystemdFd(fd_offset) => Resource::SystemFdOffset(fd_offset),
        }
    }

    /// Gets this listen address as a `Resource`, specifically for TCP.
    #[cfg(feature = "sources-utils-net-tcp")]
    pub const fn as_tcp_resource(self) -> Resource {
        self.as_resource(Protocol::Tcp)
    }

    /// Gets this listen address as a `Resource`, specifically for UDP.
    #[cfg(feature = "sources-utils-net-udp")]
    pub const fn as_udp_resource(self) -> Resource {
        self.as_resource(Protocol::Udp)
    }
}

impl fmt::Display for SocketListenAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::SocketAddr(ref addr) => addr.fmt(f),
            Self::SystemdFd(offset) => write!(f, "systemd socket #{}", offset),
        }
    }
}

impl From<SocketAddr> for SocketListenAddr {
    fn from(addr: SocketAddr) -> Self {
        Self::SocketAddr(addr)
    }
}

fn parse_systemd_fd<'de, D>(des: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &'de str = Deserialize::deserialize(des)?;
    match s {
        "systemd" => Ok(0),
        s if s.starts_with("systemd#") => s[8..]
            .parse::<usize>()
            .map_err(de::Error::custom)?
            .checked_sub(1)
            .ok_or_else(|| de::Error::custom("systemd indices start from 1, found 0")),
        _ => Err(de::Error::custom("must start with \"systemd\"")),
    }
}

#[cfg(test)]
mod test {
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    use serde::Deserialize;

    use super::SocketListenAddr;

    #[derive(Debug, Deserialize)]
    struct Config {
        addr: SocketListenAddr,
    }

    #[test]
    fn parse_socket_listen_addr() {
        let test: Config = toml::from_str(r#"addr="127.1.2.3:1234""#).unwrap();
        assert_eq!(
            test.addr,
            SocketListenAddr::SocketAddr(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::new(127, 1, 2, 3),
                1234,
            )))
        );
        let test: Config = toml::from_str(r#"addr="systemd""#).unwrap();
        assert_eq!(test.addr, SocketListenAddr::SystemdFd(0));
        let test: Config = toml::from_str(r#"addr="systemd#3""#).unwrap();
        assert_eq!(test.addr, SocketListenAddr::SystemdFd(2));
    }
}
