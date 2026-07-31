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

    /// Removed in 0.55.0. Accepted but ignored for backwards compatibility.
    ///
    /// The GraphQL Playground UI was removed when the observability API migrated
    /// from GraphQL to gRPC. Setting this option has no effect; remove it from
    /// your configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[configurable(deprecated)]
    #[configurable(metadata(docs::hidden))]
    pub playground: Option<bool>,

    /// Removed in 0.55.0. Accepted but ignored for backwards compatibility.
    ///
    /// The GraphQL endpoint was removed when the observability API migrated to
    /// gRPC. Setting this option has no effect; remove it from your
    /// configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[configurable(deprecated)]
    #[configurable(metadata(docs::hidden))]
    pub graphql: Option<bool>,
}

impl_generate_config_from_default!(Options);

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            address: default_address(),
            playground: None,
            graphql: None,
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
            playground: self.playground.or(other.playground),
            graphql: self.graphql.or(other.graphql),
        };

        *self = options;
        Ok(())
    }

    /// Check deprecated, ignored fields the user has set.
    ///
    /// `playground` and `graphql` were removed in 0.55.0 along with the GraphQL
    /// observability API. They are still accepted at deserialize time so configs
    /// that carry them don't break on upgrade, but the values have no effect.
    /// While in the warn window (see [`crate::config::deprecation`]) a `warn!`
    /// fires; once past the window each set field becomes a config-load error.
    ///
    /// Returns `Err` with one message per still-set field that has reached the
    /// error stage, so callers can surface every failure in a single load
    /// attempt.
    pub fn check_deprecated_fields(&self) -> Result<(), Vec<String>> {
        use crate::config::deprecation::{VectorMinor, check_deprecated_field};

        const REMOVED_IN: VectorMinor = VectorMinor::new(0, 55);
        let mut errors = Vec::new();

        if self.playground.is_some()
            && let Err(e) = check_deprecated_field(
                "api.playground",
                REMOVED_IN,
                "The GraphQL Playground was removed when the observability API migrated to gRPC.",
            )
        {
            errors.push(e);
        }
        if self.graphql.is_some()
            && let Err(e) = check_deprecated_field(
                "api.graphql",
                REMOVED_IN,
                "The GraphQL endpoint was removed when the observability API migrated to gRPC.",
            )
        {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[test]
fn bool_merge() {
    let mut a = Options {
        enabled: true,
        address: None,
        ..Options::default()
    };

    a.merge(Options::default()).unwrap();

    assert_eq!(
        a,
        Options {
            enabled: true,
            address: default_address(),
            ..Options::default()
        }
    );
}

#[test]
fn bind_merge() {
    let address = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9000);
    let mut a = Options {
        enabled: true,
        address: Some(address),
        ..Options::default()
    };

    a.merge(Options::default()).unwrap();

    assert_eq!(
        a,
        Options {
            enabled: true,
            address: Some(address),
            ..Options::default()
        }
    );
}

#[test]
fn deprecated_fields_default_to_none() {
    let opts = Options::default();
    assert!(opts.playground.is_none());
    assert!(opts.graphql.is_none());
}

#[test]
fn deprecated_fields_round_trip_through_toml() {
    // Setting either deprecated field must not be a config error.
    let opts: Options = toml::from_str(
        r#"
        enabled = true
        playground = false
        graphql = false
    "#,
    )
    .expect("config with deprecated api.playground/api.graphql must still parse");
    assert_eq!(opts.playground, Some(false));
    assert_eq!(opts.graphql, Some(false));
    assert!(opts.enabled);
}

#[test]
fn deprecated_fields_carry_through_merge() {
    let mut a = Options {
        playground: Some(false),
        ..Options::default()
    };
    let b = Options {
        graphql: Some(true),
        ..Options::default()
    };
    a.merge(b).unwrap();
    assert_eq!(a.playground, Some(false));
    assert_eq!(a.graphql, Some(true));
}

#[test]
fn check_deprecated_fields_ok_when_nothing_set() {
    // Default config has neither field set; must return Ok without producing
    // any error messages (and without printing a warning, though that's a
    // tracing-side observation we don't try to assert here).
    assert!(Options::default().check_deprecated_fields().is_ok());
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
