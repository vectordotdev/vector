use std::{fmt, net::SocketAddr};

use http::Uri;

/// A gRPC address.
///
/// This wrapper provides the ability to use a simple `SocketAddr` and generate the appropriate
/// output form of the address depending on whether or not the address is being used by a gRPC
/// client or gRPC server.
#[derive(Clone, Debug)]
pub struct GrpcAddress {
    addr: SocketAddr,
}

impl GrpcAddress {
    /// Gets the socket address.
    ///
    /// This is typically used when actually binding a socket to use for listening for connections
    /// as a gRPC server.
    pub const fn as_socket_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Gets the fully-qualified endpoint address.
    ///
    /// This is a URI in the form of `http://<socket address>/`. The scheme and path are hard-coded.
    pub fn as_uri(&self) -> Uri {
        let addr_str = self.addr.to_string();
        Uri::builder()
            .scheme("http")
            .authority(addr_str)
            .path_and_query("/")
            .build()
            .expect("should not fail to build URI")
    }
}

impl From<SocketAddr> for GrpcAddress {
    fn from(addr: SocketAddr) -> Self {
        Self { addr }
    }
}

impl fmt::Display for GrpcAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.addr.fmt(f)
    }
}
