//! Handles enrichment tables for `type = geoip`.
//! Enrichment data is loaded from one of the MaxMind GeoIP databases,
//! [MaxMind GeoIP2][maxmind] or [GeoLite2 binary city database][geolite].
//!
//! [maxmind]: https://dev.maxmind.com/geoip/geoip2/downloadable
//! [geolite]: https://dev.maxmind.com/geoip/geoip2/geolite2/#Download_Access
use std::{collections::BTreeMap, fs, net::IpAddr, sync::Arc, time::SystemTime};

use maxminddb::{
    geoip2::{City, ConnectionType, Isp},
    MaxMindDBError, Reader,
};
use ordered_float::NotNan;
use vector_lib::configurable::configurable_component;
use vector_lib::enrichment::{Case, Condition, IndexHandle, Table};
use vrl::value::{ObjectMap, Value};

use crate::config::{EnrichmentTableConfig, GenerateConfig};

// MaxMind GeoIP database files have a type field we can use to recognize specific
// products. If we encounter one of these two types, we look for ASN/ISP information;
// otherwise we expect to be working with a City database.
#[derive(Copy, Clone, Debug)]
#[allow(missing_docs)]
pub enum DatabaseKind {
    Asn,
    Isp,
    ConnectionType,
    City,
}

impl From<&str> for DatabaseKind {
    fn from(v: &str) -> Self {
        match v {
            "GeoLite2-ASN" => Self::Asn,
            "GeoIP2-ISP" => Self::Isp,
            "GeoIP2-Connection-Type" => Self::ConnectionType,
            _ => Self::City,
        }
    }
}

/// Configuration for the `geoip` enrichment table.
#[derive(Clone, Debug, Eq, PartialEq)]
#[configurable_component(enrichment_table("geoip"))]
pub struct GeoipConfig {
    /// Path to the [MaxMind GeoIP2][geoip2] or [GeoLite2 binary city database file][geolite2]
    /// (**GeoLite2-City.mmdb**).
    ///
    /// Other databases, such as the country database, are not supported.
    ///
    /// [geoip2]: https://dev.maxmind.com/geoip/geoip2/downloadable
    /// [geolite2]: https://dev.maxmind.com/geoip/geoip2/geolite2/#Download_Access
    pub path: String,

    /// The locale to use when querying the database.
    ///
    /// MaxMind includes localized versions of some of the fields within their database, such as
    /// country name. This setting can control which of those localized versions are returned by the
    /// transform.
    ///
    /// More information on which portions of the geolocation data are localized, and what languages
    /// are available, can be found [here][locale_docs].
    ///
    /// [locale_docs]: https://support.maxmind.com/hc/en-us/articles/4414877149467-IP-Geolocation-Data#h_01FRRGRYTGZB29ERDBZCX3MR8Q
    #[serde(default = "default_locale")]
    pub locale: String,
}

fn default_locale() -> String {
    // Valid locales at the time of writing are: "de”, "en", “es”, “fr”, “ja”, “pt-BR”, “ru”, and
    // “zh-CN”.
    //
    // More information, including the up-to-date list of locales, can be found at
    // https://dev.maxmind.com/geoip/docs/databases/city-and-country?lang=en.

    // TODO: could we detect the system locale and use that as the default locale if it matches one
    // of the available locales in the dataset, and then fallback to "en" otherwise?
    "en".to_string()
}

impl GenerateConfig for GeoipConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            path: "/path/to/GeoLite2-City.mmdb".to_string(),
            locale: default_locale(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl EnrichmentTableConfig for GeoipConfig {
    async fn build(
        &self,
        _: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn Table + Send + Sync>> {
        Ok(Box::new(Geoip::new(self.clone())?))
    }
}

#[derive(Clone)]
/// A struct that implements [vector_lib::enrichment::Table] to handle loading enrichment data from a GeoIP database.
pub struct Geoip {
    config: GeoipConfig,
    dbreader: Arc<maxminddb::Reader<Vec<u8>>>,
    dbkind: DatabaseKind,
    last_modified: SystemTime,
}

impl Geoip {
    /// Creates a new GeoIP struct from the provided config.
    pub fn new(config: GeoipConfig) -> crate::Result<Self> {
        let dbreader = Arc::new(Reader::open_readfile(config.path.clone())?);
        let dbkind = DatabaseKind::from(dbreader.metadata.database_type.as_str());

        // Check if we can read database with dummy Ip.
        let ip = IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED);
        let result = match dbkind {
            DatabaseKind::Asn | DatabaseKind::Isp => dbreader.lookup::<Isp>(ip).map(|_| ()),
            DatabaseKind::ConnectionType => dbreader.lookup::<ConnectionType>(ip).map(|_| ()),
            DatabaseKind::City => dbreader.lookup::<City>(ip).map(|_| ()),
        };

        match result {
            Ok(_) | Err(MaxMindDBError::AddressNotFoundError(_)) => Ok(Geoip {
                last_modified: fs::metadata(&config.path)?.modified()?,
                dbreader,
                dbkind,
                config,
            }),
            Err(error) => Err(error.into()),
        }
    }

    fn lookup(&self, ip: IpAddr, select: Option<&[String]>) -> Option<ObjectMap> {
        let mut map = ObjectMap::new();
        let mut add_field = |key: &str, value: Option<Value>| {
            if select
                .map(|fields| fields.iter().any(|field| field == key))
                .unwrap_or(true)
            {
                map.insert(key.into(), value.unwrap_or(Value::Null));
            }
        };

        macro_rules! add_field {
            ($k:expr, $v:expr) => {
                add_field($k, $v.map(Into::into))
            };
        }

        match self.dbkind {
            DatabaseKind::Asn | DatabaseKind::Isp => {
                let data = self.dbreader.lookup::<Isp>(ip).ok()?;

                add_field!("autonomous_system_number", data.autonomous_system_number);
                add_field!(
                    "autonomous_system_organization",
                    data.autonomous_system_organization
                );
                add_field!("isp", data.isp);
                add_field!("organization", data.organization);
            }
            DatabaseKind::City => {
                let data = self.dbreader.lookup::<City>(ip).ok()?;

                add_field!(
                    "city_name",
                    self.take_translation(data.city.as_ref().and_then(|c| c.names.as_ref()))
                );

                add_field!("continent_code", data.continent.and_then(|c| c.code));

                let country = data.country.as_ref();
                add_field!("country_code", country.and_then(|country| country.iso_code));
                add_field!(
                    "country_name",
                    self.take_translation(country.and_then(|c| c.names.as_ref()))
                );

                let location = data.location.as_ref();
                add_field!("timezone", location.and_then(|location| location.time_zone));
                add_field!(
                    "latitude",
                    location
                        .and_then(|location| location.latitude)
                        .map(|latitude| Value::Float(
                            NotNan::new(latitude).expect("latitude cannot be Nan")
                        ))
                );
                add_field!(
                    "longitude",
                    location
                        .and_then(|location| location.longitude)
                        .map(|longitude| NotNan::new(longitude).expect("longitude cannot be Nan"))
                );
                add_field!(
                    "metro_code",
                    location.and_then(|location| location.metro_code)
                );

                // last subdivision is most specific per https://github.com/maxmind/GeoIP2-java/blob/39385c6ce645374039450f57208b886cf87ade47/src/main/java/com/maxmind/geoip2/model/AbstractCityResponse.java#L96-L107
                let subdivision = data.subdivisions.as_ref().and_then(|s| s.last());
                add_field!(
                    "region_name",
                    self.take_translation(subdivision.and_then(|s| s.names.as_ref()))
                );
                add_field!(
                    "region_code",
                    subdivision.and_then(|subdivision| subdivision.iso_code)
                );
                add_field!("postal_code", data.postal.and_then(|p| p.code));
            }
            DatabaseKind::ConnectionType => {
                let data = self.dbreader.lookup::<ConnectionType>(ip).ok()?;

                add_field!("connection_type", data.connection_type);
            }
        }

        Some(map)
    }

    fn take_translation<'a>(
        &self,
        translations: Option<&BTreeMap<&str, &'a str>>,
    ) -> Option<&'a str> {
        translations
            .and_then(|translations| translations.get(&*self.config.locale))
            .copied()
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
    ) -> Result<ObjectMap, String> {
        let mut rows = self.find_table_rows(case, condition, select, index)?;

        match rows.pop() {
            Some(row) if rows.is_empty() => Ok(row),
            Some(_) => Err("More than 1 row found".to_string()),
            None => Err("IP not found".to_string()),
        }
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
    ) -> Result<Vec<ObjectMap>, String> {
        match condition.first() {
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
    fn add_index(&mut self, _: Case, fields: &[&str]) -> Result<IndexHandle, String> {
        match fields.len() {
            0 => Err("IP field is required".to_string()),
            1 => Ok(IndexHandle(0)),
            _ => Err("Only one field is allowed".to_string()),
        }
    }

    /// Returns a list of the field names that are in each index
    fn index_fields(&self) -> Vec<(Case, Vec<String>)> {
        Vec::new()
    }

    /// Returns true if the underlying data has changed and the table needs reloading.
    fn needs_reload(&self) -> bool {
        matches!(fs::metadata(&self.config.path)
            .and_then(|metadata| metadata.modified()),
            Ok(modified) if modified > self.last_modified)
    }
}

impl std::fmt::Debug for Geoip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Geoip {} database {})",
            self.config.locale, self.config.path
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn city_lookup() {
        let values = find("2.125.160.216", "tests/data/GeoIP2-City-Test.mmdb").unwrap();

        let mut expected = ObjectMap::new();
        expected.insert("city_name".into(), "Boxford".into());
        expected.insert("country_code".into(), "GB".into());
        expected.insert("continent_code".into(), "EU".into());
        expected.insert("country_name".into(), "United Kingdom".into());
        expected.insert("region_code".into(), "WBK".into());
        expected.insert("region_name".into(), "West Berkshire".into());
        expected.insert("timezone".into(), "Europe/London".into());
        expected.insert("latitude".into(), Value::from(51.75));
        expected.insert("longitude".into(), Value::from(-1.25));
        expected.insert("postal_code".into(), "OX1".into());
        expected.insert("metro_code".into(), Value::Null);

        assert_eq!(values, expected);
    }

    #[test]
    fn city_partial_lookup() {
        let values = find_select(
            "2.125.160.216",
            "tests/data/GeoIP2-City-Test.mmdb",
            Some(&["latitude".to_string(), "longitude".to_string()]),
        )
        .unwrap();

        let mut expected = ObjectMap::new();
        expected.insert("latitude".into(), Value::from(51.75));
        expected.insert("longitude".into(), Value::from(-1.25));

        assert_eq!(values, expected);
    }

    #[test]
    fn city_lookup_partial_results() {
        let values = find("67.43.156.9", "tests/data/GeoIP2-City-Test.mmdb").unwrap();

        let mut expected = ObjectMap::new();
        expected.insert("city_name".into(), Value::Null);
        expected.insert("country_code".into(), "BT".into());
        expected.insert("country_name".into(), "Bhutan".into());
        expected.insert("continent_code".into(), "AS".into());
        expected.insert("region_code".into(), Value::Null);
        expected.insert("region_name".into(), Value::Null);
        expected.insert("timezone".into(), "Asia/Thimphu".into());
        expected.insert("latitude".into(), Value::from(27.5));
        expected.insert("longitude".into(), Value::from(90.5));
        expected.insert("postal_code".into(), Value::Null);
        expected.insert("metro_code".into(), Value::Null);

        assert_eq!(values, expected);
    }

    #[test]
    fn city_lookup_no_results() {
        let values = find("10.1.12.1", "tests/data/GeoIP2-City-Test.mmdb");

        assert!(values.is_none());
    }

    #[test]
    fn isp_lookup() {
        let values = find("208.192.1.2", "tests/data/GeoIP2-ISP-Test.mmdb").unwrap();

        let mut expected = ObjectMap::new();
        expected.insert("autonomous_system_number".into(), 701i64.into());
        expected.insert(
            "autonomous_system_organization".into(),
            "MCI Communications Services, Inc. d/b/a Verizon Business".into(),
        );
        expected.insert("isp".into(), "Verizon Business".into());
        expected.insert("organization".into(), "Verizon Business".into());

        assert_eq!(values, expected);
    }

    #[test]
    fn isp_lookup_partial_results() {
        let values = find("2600:7000::1", "tests/data/GeoLite2-ASN-Test.mmdb").unwrap();

        let mut expected = ObjectMap::new();
        expected.insert("autonomous_system_number".into(), 6939i64.into());
        expected.insert(
            "autonomous_system_organization".into(),
            "Hurricane Electric, Inc.".into(),
        );
        expected.insert("isp".into(), Value::Null);
        expected.insert("organization".into(), Value::Null);

        assert_eq!(values, expected);
    }

    #[test]
    fn isp_lookup_no_results() {
        let values = find("10.1.12.1", "tests/data/GeoLite2-ASN-Test.mmdb");

        assert!(values.is_none());
    }

    #[test]
    fn connection_type_lookup_success() {
        let values = find(
            "201.243.200.1",
            "tests/data/GeoIP2-Connection-Type-Test.mmdb",
        )
        .unwrap();

        let mut expected = ObjectMap::new();
        expected.insert("connection_type".into(), "Corporate".into());

        assert_eq!(values, expected);
    }

    #[test]
    fn connection_type_lookup_missing() {
        let values = find("10.1.12.1", "tests/data/GeoIP2-Connection-Type-Test.mmdb");

        assert!(values.is_none());
    }

    fn find(ip: &str, database: &str) -> Option<ObjectMap> {
        find_select(ip, database, None)
    }

    fn find_select(ip: &str, database: &str, select: Option<&[String]>) -> Option<ObjectMap> {
        Geoip::new(GeoipConfig {
            path: database.to_string(),
            locale: default_locale(),
        })
        .unwrap()
        .find_table_rows(
            Case::Insensitive,
            &[Condition::Equals {
                field: "ip",
                value: ip.into(),
            }],
            select,
            None,
        )
        .unwrap()
        .pop()
    }
}
