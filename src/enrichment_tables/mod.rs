pub use enrichment::{Condition, IndexHandle, Table};
use enum_dispatch::enum_dispatch;
use vector_config::{configurable_component, NamedComponent};

use crate::config::{EnrichmentTableConfig, GlobalOptions};

pub mod file;

#[cfg(feature = "enrichment-tables-geoip")]
pub mod geoip;

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
}

// We can't use `enum_dispatch` here because it doesn't support associated constants.
impl NamedComponent for EnrichmentTables {
    const NAME: &'static str = "_invalid_usage";

    fn get_component_name(&self) -> &'static str {
        match self {
            Self::File(config) => config.get_component_name(),
            #[cfg(feature = "enrichment-tables-geoip")]
            Self::Geoip(config) => config.get_component_name(),
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }
}
