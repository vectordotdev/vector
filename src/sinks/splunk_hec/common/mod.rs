pub mod acknowledgements;
pub mod request;
pub mod response;
pub mod service;
pub mod util;

use serde::{Deserialize, Serialize};
pub use util::*;

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EndpointTarget {
    Raw,
    Event,
}

impl Default for EndpointTarget {
    fn default() -> Self {
        EndpointTarget::Event
    }
}
