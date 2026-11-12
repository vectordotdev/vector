#![deny(warnings)]
#![deny(clippy::all)]
#![deny(unreachable_pub)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]

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
