//! Functionality to handle enrichment tables.
use enum_dispatch::enum_dispatch;
use vector_lib::configurable::configurable_component;
pub use vector_lib::enrichment::{Condition, IndexHandle, Table};

use crate::config::{
    ComponentKey, EnrichmentTableConfig, GenerateConfig, GlobalOptions, SinkConfig, SourceConfig,
};

pub mod file;

#[cfg(feature = "enrichment-tables-memory")]
pub mod memory;

#[cfg(feature = "enrichment-tables-geoip")]
pub mod geoip;

#[cfg(feature = "enrichment-tables-mmdb")]
pub mod mmdb;

/// Configuration options for an [enrichment table](https://vector.dev/docs/reference/glossary/#enrichment-tables) to be used in a
/// [`remap`](https://vector.dev/docs/reference/configuration/transforms/remap/) transform. Currently supported are:
///
/// * [CSV](https://en.wikipedia.org/wiki/Comma-separated_values) files
/// * [MaxMind](https://www.maxmind.com/en/home) databases
/// * In-memory storage
///
/// For the lookup in the enrichment tables to be as performant as possible, the data is indexed according
/// to the fields that are used in the search. Note that indices can only be created for fields for which an
/// exact match is used in the condition. For range searches, an index isn't used and the enrichment table
/// drops back to a sequential scan of the data. A sequential scan shouldn't impact performance
/// significantly provided that there are only a few possible rows returned by the exact matches in the
/// condition. We don't recommend using a condition that uses only date range searches.
///
///
#[configurable_component(global_option("enrichment_tables"))]
#[derive(Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
#[enum_dispatch(EnrichmentTableConfig)]
#[configurable(metadata(
    docs::enum_tag_description = "enrichment table type",
    docs::common = false,
    docs::required = false,
))]
pub enum EnrichmentTables {
    /// Exposes data from a static file as an enrichment table.
    File(file::FileConfig),

    /// Exposes data from a memory cache as an enrichment table. The cache can be written to using
    /// a sink.
    #[cfg(feature = "enrichment-tables-memory")]
    Memory(memory::MemoryConfig),

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
}

impl GenerateConfig for EnrichmentTables {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::File(file::FileConfig {
            file: file::FileSettings {
                path: "path/to/file".into(),
                encoding: file::Encoding::default(),
            },
            schema: Default::default(),
        }))
        .unwrap()
    }
}
