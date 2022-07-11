pub use enrichment::{Condition, IndexHandle, Table};

#[cfg(feature = "enrichment-tables-file")]
pub mod file;

#[cfg(feature = "enrichment-tables-geoip")]
pub mod geoip;
