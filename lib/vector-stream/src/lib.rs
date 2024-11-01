#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(unreachable_pub)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::type_complexity)]

pub mod batcher;
mod concurrent_map;
mod driver;
pub mod expiration_map;
mod futures_unordered_count;
mod partitioned_batcher;

pub use concurrent_map::ConcurrentMap;
pub use driver::{Driver, DriverResponse};
use futures_unordered_count::FuturesUnorderedCount;
pub use partitioned_batcher::{BatcherSettings, ExpirationQueue, PartitionedBatcher};

#[macro_use]
extern crate tracing;
