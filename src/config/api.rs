use std::net::{Ipv4Addr, SocketAddr};

use url::Url;
use vector_lib::configurable::configurable_component;

/// API options.
#[configurable_component]
#[configurable(metadata(
    docs::warnings = "The API currently does not support authentication. Only enable it in isolated environments or for debugging. It must not be exposed to untrusted clients."
))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    /// Whether the API is enabled for this Vector instance.
    #[serde(default = "default_enabled")]
    #[configurable(metadata(docs::common = true, docs::required = false))]
    pub enabled: bool,

    /// The network address to which the API should bind. If you're running
    /// Vector in a Docker container, bind to `0.0.0.0`. Otherwise
    /// the API will not be exposed outside the container.
    #[serde(default = "default_address")]
    #[configurable(metadata(docs::examples = "0.0.0.0:8686"))]
    #[configurable(metadata(docs::examples = "127.0.0.1:1234"))]
    #[configurable(metadata(docs::common = true, docs::required = false))]
    pub address: Option<SocketAddr>,
}

impl_generate_config_from_default!(Options);

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            address: default_address(),
        }
    }
}

const fn default_enabled() -> bool {
    false
}

/// By default, the API binds to 127.0.0.1:8686. This function should remain public;
/// `vector top`  will use it to determine which to connect to by default, if no URL
/// override is provided.
pub fn default_address() -> Option<SocketAddr> {
    Some(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8686))
}

/// Default gRPC API address for `vector top` and other API clients
pub fn default_grpc_url() -> Url {
    let addr = default_address().unwrap();
    Url::parse(&format!("http://{addr}"))
        .expect("Couldn't parse default API URL. Please report this.")
}

impl Options {
    pub fn merge(&mut self, other: Self) -> Result<(), String> {
        // Merge options

        // Try to merge address
        let address = match (self.address, other.address) {
            (None, b) => b,
            (Some(a), None) => Some(a),
            (Some(a), Some(b)) if a == b => Some(a),
            // Prefer non default address
            (Some(a), Some(b)) => {
                match (Some(a) == default_address(), Some(b) == default_address()) {
                    (false, false) => return Err(format!("Conflicting `api` address: {a}, {b} .")),
                    (false, true) => Some(a),
                    (true, _) => Some(b),
                }
            }
        };

        let options = Options {
            address,
            enabled: self.enabled | other.enabled,
        };

        *self = options;
        Ok(())
    }
}

#[test]
fn bool_merge() {
    let mut a = Options {
        enabled: true,
        address: None,
    };

    a.merge(Options::default()).unwrap();

    assert_eq!(
        a,
        Options {
            enabled: true,
            address: default_address(),
        }
    );
}

#[test]
fn bind_merge() {
    let address = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9000);
    let mut a = Options {
        enabled: true,
        address: Some(address),
    };

    a.merge(Options::default()).unwrap();

    assert_eq!(
        a,
        Options {
            enabled: true,
            address: Some(address),
        }
    );
}

#[test]
fn bind_conflict() {
    let mut a = Options {
        address: Some(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9000)),
        ..Options::default()
    };

    let b = Options {
        address: Some(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9001)),
        ..Options::default()
    };

    assert!(a.merge(b).is_err());
}
