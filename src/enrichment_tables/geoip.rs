use std::{collections::BTreeMap, fs, net::IpAddr, sync::Arc, time::SystemTime};

use enrichment::{Case, Condition, IndexHandle, Table};
use serde::{Deserialize, Serialize};
use value::Value;

use crate::config::{EnrichmentTableConfig, EnrichmentTableDescription};

// MaxMind GeoIP database files have a type field we can use to recognize specific
// products. If we encounter one of these two types, we look for ASN/ISP information;
// otherwise we expect to be working with a City database.
const ASN_DATABASE_TYPE: &str = "GeoLite2-ASN";
const ISP_DATABASE_TYPE: &str = "GeoIP2-ISP";

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct GeoipConfig {
    pub database: String,
    #[serde(default = "default_locale")]
    pub locale: String,
}

// valid locales are: “de”, "en", “es”, “fr”, “ja”, “pt-BR”, “ru”, and “zh-CN”
//
// https://dev.maxmind.com/geoip/docs/databases/city-and-country?lang=en
//
// TODO try to determine the system locale and use that as default if it matches a valid locale?
fn default_locale() -> String {
    "en".to_string()
}

#[async_trait::async_trait]
#[typetag::serde(name = "geoip")]
impl EnrichmentTableConfig for GeoipConfig {
    async fn build(
        &self,
        _: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn Table + Send + Sync>> {
        Ok(Box::new(Geoip::new(self.clone())?))
    }
}

inventory::submit! {
    EnrichmentTableDescription::new::<GeoipConfig>("geoip")
}

impl_generate_config_from_default!(GeoipConfig);

#[derive(Clone)]
pub struct Geoip {
    config: GeoipConfig,
    dbreader: Arc<maxminddb::Reader<Vec<u8>>>,
    last_modified: SystemTime,
    indexes: Vec<(Case, Vec<String>)>,
}

impl Geoip {
    pub fn new(config: GeoipConfig) -> crate::Result<Self> {
        Ok(Geoip {
            last_modified: fs::metadata(&config.database)?.modified()?,
            dbreader: Arc::new(maxminddb::Reader::open_readfile(config.database.clone())?),
            config,
            indexes: Vec::new(),
        })
    }

    fn has_isp_db(&self) -> bool {
        self.dbreader.metadata.database_type == ASN_DATABASE_TYPE
            || self.dbreader.metadata.database_type == ISP_DATABASE_TYPE
    }

    fn lookup(&self, ip: IpAddr, select: Option<&[String]>) -> Option<BTreeMap<String, Value>> {
        let mut map = BTreeMap::new();
        let mut add_field = |key: &str, value: Option<Value>| {
            if select
                .map(|fields| fields.iter().any(|field| field == key))
                .unwrap_or(true)
            {
                map.insert(key.to_string(), value.unwrap_or(Value::Null));
            }
        };

        if self.has_isp_db() {
            let data = self.dbreader.lookup::<maxminddb::geoip2::Isp>(ip).ok()?;

            add_field(
                "autonomous_system_number",
                data.autonomous_system_number.map(Into::into),
            );
            add_field(
                "autonomous_system_organization",
                data.autonomous_system_organization.map(Into::into),
            );

            add_field("isp", data.isp.map(Into::into));

            add_field("organization", data.organization.map(Into::into));
        } else {
            let data = self.dbreader.lookup::<maxminddb::geoip2::City>(ip).ok()?;

            add_field(
                "city_name",
                data.city
                    .as_ref()
                    .and_then(|c| c.names.as_ref())
                    .and_then(|names| names.get(&*self.config.locale))
                    .map(|&name| name.into()),
            );

            add_field(
                "continent_code",
                data.continent.and_then(|c| c.code).map(Into::into),
            );

            let country = data.country.as_ref();
            add_field(
                "country_code",
                country.and_then(|country| country.iso_code).map(Into::into),
            );
            add_field(
                "country_name",
                country
                    .and_then(|country| {
                        country
                            .names
                            .as_ref()
                            .and_then(|names| names.get(&*self.config.locale))
                    })
                    .map(|&name| name.into()),
            );

            let location = data.location.as_ref();
            add_field(
                "timezone",
                location
                    .and_then(|location| location.time_zone)
                    .map(Into::into),
            );
            add_field(
                "latitude",
                location
                    .and_then(|location| location.latitude)
                    .map(Into::into),
            );
            add_field(
                "longitude",
                location
                    .and_then(|location| location.longitude)
                    .map(Into::into),
            );
            add_field(
                "metro_code",
                location
                    .and_then(|location| location.metro_code)
                    .map(Into::into),
            );

            // last subdivision is most specific per https://github.com/maxmind/GeoIP2-java/blob/39385c6ce645374039450f57208b886cf87ade47/src/main/java/com/maxmind/geoip2/model/AbstractCityResponse.java#L96-L107
            let subdivision = data.subdivisions.as_ref().and_then(|s| s.last());
            add_field(
                "region_name",
                subdivision
                    .and_then(|subdivision| {
                        subdivision
                            .names
                            .as_ref()
                            .and_then(|names| names.get(&*self.config.locale))
                    })
                    .map(|&name| name.into()),
            );
            add_field(
                "region_code",
                subdivision
                    .and_then(|subdivision| subdivision.iso_code)
                    .map(Into::into),
            );

            add_field(
                "postal_code",
                data.postal.and_then(|p| p.code).map(Into::into),
            );
        }

        Some(map)
    }
}

impl Table for Geoip {
    /// Search the enrichment table data with the given condition.
    /// All conditions must match (AND).
    ///
    /// # Errors
    /// Errors if no rows, or more than 1 row is found.
    fn find_table_row<'a>(
        &self,
        case: Case,
        condition: &'a [Condition<'a>],
        select: Option<&[String]>,
        index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, Value>, String> {
        self.find_table_rows(case, condition, select, index)?
            .pop()
            .ok_or_else(|| "IP not found".to_string())
    }

    /// Search the enrichment table data with the given condition.
    /// All conditions must match (AND).
    /// Can return multiple matched records
    fn find_table_rows<'a>(
        &self,
        _: Case,
        condition: &'a [Condition<'a>],
        select: Option<&[String]>,
        _: Option<IndexHandle>,
    ) -> Result<Vec<BTreeMap<String, Value>>, String> {
        match condition.get(0) {
            Some(_) if condition.len() > 1 => Err("Only one condition is allowed".to_string()),
            Some(Condition::Equals { value, .. }) => {
                let ip = value
                    .to_string_lossy()
                    .parse::<IpAddr>()
                    .map_err(|_| "Invalid IP address".to_string())?;
                Ok(self
                    .lookup(ip, select)
                    .map(|values| vec![values])
                    .unwrap_or_default())
            }
            Some(_) => Err("Only equality condition is allowed".to_string()),
            None => Err("IP condition must be specified".to_string()),
        }
    }

    /// Hints to the enrichment table what data is going to be searched to allow it to index the
    /// data in advance.
    ///
    /// # Errors
    /// Errors if the fields are not in the table.
    fn add_index(&mut self, case: Case, fields: &[&str]) -> Result<IndexHandle, String> {
        match fields.len() {
            0 => Err("IP field is required".to_string()),
            1 => {
                let index = IndexHandle(self.indexes.len());
                self.indexes
                    .push((case, fields.iter().map(|field| field.to_string()).collect()));
                Ok(index)
            }
            _ => Err("Only one field is allowed".to_string()),
        }
    }

    /// Returns a list of the field names that are in each index
    fn index_fields(&self) -> Vec<(Case, Vec<String>)> {
        self.indexes.clone()
    }

    /// Returns true if the underlying data has changed and the table needs reloading.
    fn needs_reload(&self) -> bool {
        matches!(fs::metadata(&self.config.database)
            .and_then(|metadata| metadata.modified()),
            Ok(modified) if modified > self.last_modified)
    }
}

impl std::fmt::Debug for Geoip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Geoip {} database {})",
            self.config.locale, self.config.database
        )
    }
}
