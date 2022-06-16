pub mod acknowledgements;
pub mod request;
pub mod response;
pub mod service;
pub mod util;

use serde::{Deserialize, Serialize};
pub use util::*;

pub(super) const SOURCE_FIELD: &str = "source";
pub(super) const SOURCETYPE_FIELD: &str = "sourcetype";
pub(super) const INDEX_FIELD: &str = "index";
pub(super) const HOST_FIELD: &str = "host";

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EndpointTarget {
    Raw,
    Event,
}

impl Default for EndpointTarget {
    fn default() -> Self {
        EndpointTarget::Event
    }
}
