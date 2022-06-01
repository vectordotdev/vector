pub mod acknowledgements;
pub mod request;
pub mod response;
pub mod service;
pub mod util;

pub use util::*;

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
