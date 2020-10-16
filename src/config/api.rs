use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr};

#[derive(Debug, Deserialize, Serialize, PartialEq, Copy, Clone)]
#[serde(default)]
pub struct Options {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_bind")]
    pub bind: Option<SocketAddr>,

    #[serde(default = "default_playground")]
    pub playground: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            playground: default_playground(),
            bind: default_bind(),
        }
    }
}

fn default_enabled() -> bool {
    false
}

fn default_bind() -> Option<SocketAddr> {
    Some(SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8686))
}

fn default_playground() -> bool {
    true
}

impl Options {
    pub fn merge(&mut self, other: Self) -> Result<(), String> {
        // Merge options

        // Try to merge bind
        let bind = match (self.bind, other.bind) {
            (None, b) => b,
            (Some(a), None) => Some(a),
            (Some(a), Some(b)) if a == b => Some(a),
            // Prefer non default bind
            (Some(a), Some(b)) => match (Some(a) == default_bind(), Some(b) == default_bind()) {
                (false, false) => {
                    return Err(format!("Conflicting `api` bindings: {}, {} .", a, b))
                }
                (false, true) => Some(a),
                (true, _) => Some(b),
            },
        };

        let options = Options {
            bind,
            enabled: self.enabled | other.enabled,
            playground: self.playground & other.playground,
        };

        *self = options;
        Ok(())
    }
}

#[test]
fn bool_merge() {
    let mut a = Options {
        enabled: true,
        bind: None,
        playground: false,
    };

    a.merge(Options::default()).unwrap();

    assert_eq!(
        a,
        Options {
            enabled: true,
            bind: default_bind(),
            playground: false,
        }
    );
}

#[test]
fn bind_merge() {
    let address = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 9000);
    let mut a = Options {
        enabled: true,
        bind: Some(address),
        playground: true,
    };

    a.merge(Options::default()).unwrap();

    assert_eq!(
        a,
        Options {
            enabled: true,
            bind: Some(address),
            playground: true,
        }
    );
}

#[test]
fn bind_conflict() {
    let mut a = Options {
        bind: Some(SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 9000)),
        ..Options::default()
    };

    let b = Options {
        bind: Some(SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 9001)),
        ..Options::default()
    };

    assert!(a.merge(b).is_err());
}
