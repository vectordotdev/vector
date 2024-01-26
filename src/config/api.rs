use std::net::{Ipv4Addr, SocketAddr};

use url::Url;
use vector_lib::configurable::configurable_component;

/// API options.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    /// Whether or not the API endpoint is available.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// The socket address to listen on for the API endpoint.
    #[serde(default = "default_address")]
    pub address: Option<SocketAddr>,

    /// Whether or not to expose the GraphQL playground on the API endpoint.
    #[serde(default = "default_playground")]
    pub playground: bool,

    /// Whether or not the GraphQL endpoint is enabled
    #[serde(default = "default_graphql", skip_serializing_if = "is_true")]
    pub graphql: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            playground: default_playground(),
            address: default_address(),
            graphql: default_graphql(),
        }
    }
}

// serde passes struct fields as reference
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_true(value: &bool) -> bool {
    *value
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

/// Default GraphQL API address
pub fn default_graphql_url() -> Url {
    let addr = default_address().unwrap();
    Url::parse(&format!("http://{}/graphql", addr))
        .expect("Couldn't parse default API URL. Please report this.")
}

const fn default_playground() -> bool {
    true
}

const fn default_graphql() -> bool {
    true
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
                    (false, false) => {
                        return Err(format!("Conflicting `api` address: {}, {} .", a, b))
                    }
                    (false, true) => Some(a),
                    (true, _) => Some(b),
                }
            }
        };

        let options = Options {
            address,
            enabled: self.enabled | other.enabled,
            playground: self.playground & other.playground,
            graphql: self.graphql & other.graphql,
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
        playground: false,
        graphql: false,
    };

    a.merge(Options::default()).unwrap();

    assert_eq!(
        a,
        Options {
            enabled: true,
            address: default_address(),
            playground: false,
            graphql: false
        }
    );
}

#[test]
fn bind_merge() {
    let address = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9000);
    let mut a = Options {
        enabled: true,
        address: Some(address),
        playground: true,
        graphql: true,
    };

    a.merge(Options::default()).unwrap();

    assert_eq!(
        a,
        Options {
            enabled: true,
            address: Some(address),
            playground: true,
            graphql: true,
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
