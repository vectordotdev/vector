//! Functionality to handle enrichment tables.
use enum_dispatch::enum_dispatch;
use vector_lib::configurable::{configurable_component, NamedComponent};
pub use vector_lib::enrichment::{Condition, IndexHandle, Table};

use crate::config::{EnrichmentTableConfig, GlobalOptions};

pub mod pgtable;

pub mod file;

#[cfg(feature = "enrichment-tables-geoip")]
pub mod geoip;

#[cfg(feature = "enrichment-tables-mmdb")]
pub mod mmdb;

#[cfg(feature = "enrichment-tables-pgtable")]
pub mod pgtable;

/// Configurable enrichment tables.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
#[enum_dispatch(EnrichmentTableConfig)]
pub enum EnrichmentTables {
    /// Exposes data from a static file as an enrichment table.
    File(file::FileConfig),

    /// Exposes data from a [MaxMind][maxmind] [GeoIP2][geoip2] database as an enrichment table.
    ///
    /// [maxmind]: https://www.maxmind.com/
    /// [geoip2]: https://www.maxmind.com/en/geoip2-databases
    #[cfg(feature = "enrichment-tables-geoip")]
    Geoip(geoip::GeoipConfig),

    /// Exposes data from a [MaxMind][maxmind] database as an enrichment table.
    ///
    /// [maxmind]: https://www.maxmind.com/
    #[cfg(feature = "enrichment-tables-mmdb")]
    Mmdb(mmdb::MmdbConfig),

    /// Exposes data from a postgres table.
    #[cfg(feature = "enrichment-tables-postgres")]
    Pgtable(pgtable::PgtableConfig),
}

// TODO: Use `enum_dispatch` here.
impl NamedComponent for EnrichmentTables {
    fn get_component_name(&self) -> &'static str {
        match self {
            Self::File(config) => config.get_component_name(),
            #[cfg(feature = "enrichment-tables-geoip")]
            Self::Geoip(config) => config.get_component_name(),
            #[cfg(feature = "enrichment-tables-mmdb")]
            Self::Mmdb(config) => config.get_component_name(),
            #[cfg(feature = "enrichment-tables-postgres")]
            Self::Pgtable(config) => config.get_component_name(),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }
}
