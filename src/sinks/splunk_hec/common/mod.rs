pub mod acknowledgements;
pub mod request;
pub mod response;
pub mod service;
pub mod util;

use serde::{Deserialize, Serialize};
pub use util::*;

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub enum Data {
    Raw,
    Event,
}
