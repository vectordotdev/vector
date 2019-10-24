use super::Transform;

use crate::{
    event::Event,
    topology::config::{DataType, TransformConfig},
};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

use indexmap::IndexMap;
use std::net::IpAddr;
use std::str::FromStr;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct GeoipConfig {
    pub source: Atom,
    pub database: String,
    #[serde(default = "default_geoip_target_field")]
    pub target: String,
}

pub struct Geoip {
    pub dbreader: maxminddb::Reader<Vec<u8>>,
    pub source: Atom,
    pub target: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct GeoipDecodedData {
    pub city_name: String,
    pub continent_code: String,
    pub country_code: String,
    pub time_zone: String,
    pub latitude: String,
    pub longitude: String,
    pub postal_code: String,
}

fn default_geoip_target_field() -> String {
    "geoip".to_string()
}

#[typetag::serde(name = "geoip")]
impl TransformConfig for GeoipConfig {
    fn build(&self) -> Result<Box<dyn Transform>, crate::Error> {
        let reader = maxminddb::Reader::open_readfile(self.database.clone()).unwrap();
        Ok(Box::new(Geoip::new(
            reader,
            self.source.clone(),
            self.target.clone(),
        )))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

impl Geoip {
    pub fn new(dbreader: maxminddb::Reader<Vec<u8>>, source: Atom, target: String) -> Self {
        Geoip {
            dbreader: dbreader,
            source: source,
            target: target,
        }
    }
}

impl Transform for Geoip {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let ipaddress = event
            .as_log()
            .get(&self.source)
            .map(|s| s.to_string_lossy());
        if let Some(ipaddress) = &ipaddress {
            let mut lookup_results = IndexMap::new();
            let ip: IpAddr = FromStr::from_str(ipaddress).unwrap();
            let v = self.dbreader.lookup(ip);
            if v.is_ok() {
                let data: maxminddb::geoip2::City = v.unwrap();
                let city = data.city;
                if let Some(city) = city {
                    let city_names = city.names;
                    if let Some(city_names) = city_names {
                        let city_name_en = city_names.get("en");
                        if let Some(city_name_en) = city_name_en {
                            lookup_results.insert(Atom::from("city_name"), city_name_en.into());
                        }
                    }
                }
                let continent_code = data.continent.and_then(|c| c.code);
                if let Some(continent_code) = continent_code {
                    lookup_results.insert(Atom::from("continent_code"), continent_code.into());
                }

                let iso_code = data.country.and_then(|cy| cy.iso_code);
                if let Some(iso_code) = iso_code {
                    lookup_results.insert(Atom::from("country_code"), iso_code.into());
                }

                let time_zone = data.location.clone().and_then(|loc| loc.time_zone);
                if let Some(time_zone) = time_zone {
                    lookup_results.insert(Atom::from("time_zone"), time_zone.to_string());
                }

                let latitude = data.location.clone().and_then(|loc| loc.latitude);
                if let Some(latitude) = latitude {
                    lookup_results.insert(Atom::from("latitude"), latitude.to_string());
                }

                let longitude = data.location.clone().and_then(|loc| loc.longitude);
                if let Some(longitude) = longitude {
                    lookup_results.insert(Atom::from("longitude"), longitude.to_string());
                }

                let postal_code = data.postal.clone().and_then(|p| p.code);
                if let Some(postal_code) = postal_code {
                    lookup_results.insert(Atom::from("postal_code"), postal_code.into());
                }
            }

            let geoipdata = GeoipDecodedData {
                city_name: lookup_results
                    .get(&Atom::from("city_name"))
                    .unwrap_or(&String::from(""))
                    .to_string(),
                continent_code: lookup_results
                    .get(&Atom::from("continent_code"))
                    .unwrap_or(&String::from(""))
                    .to_string(),
                country_code: lookup_results
                    .get(&Atom::from("country_code"))
                    .unwrap_or(&String::from(""))
                    .to_string(),
                time_zone: lookup_results
                    .get(&Atom::from("time_zone"))
                    .unwrap_or(&String::from(""))
                    .to_string(),
                latitude: lookup_results
                    .get(&Atom::from("latitude"))
                    .unwrap_or(&String::from(""))
                    .to_string(),
                longitude: lookup_results
                    .get(&Atom::from("longitude"))
                    .unwrap_or(&String::from(""))
                    .to_string(),
                postal_code: lookup_results
                    .get(&Atom::from("postal_code"))
                    .unwrap_or(&String::from(""))
                    .to_string(),
            };
            event.as_mut_log().insert_explicit(
                Atom::from(self.target.clone()),
                serde_json::to_string(&geoipdata).unwrap().into(), //FIXME: handle pnic heere
            );
        } else {
            debug!(
                message = "Field does not exist.",
                field = self.source.as_ref(),
            );
        };

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::Geoip;
    use super::GeoipDecodedData;
    use crate::{
        event::Event,
        transforms::json_parser::{JsonParser, JsonParserConfig},
        transforms::Transform,
    };
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn geoip_lookup_success() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "49.255.14.118", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();
        let reader = maxminddb::Reader::open_readfile("test-data/GeoLite2-City.mmdb").unwrap();

        let mut augment = Geoip::new(reader, Atom::from("remote_addr"), "geo".to_string());
        let new_event = augment.transform(event).unwrap();

        let geodata_k = Atom::from("geo".to_string());
        let geodata = new_event.as_log().get(&geodata_k);

        let geodata_s = geodata.unwrap().to_string_lossy();
        let g: GeoipDecodedData = serde_json::from_str(&geodata_s).unwrap();

        assert_eq!(g.city_name, "Sydney");
        assert_eq!(g.country_code, "AU");
        assert_eq!(g.continent_code, "OC");
        assert_eq!(g.time_zone, "Australia/Sydney");
        assert_eq!(g.latitude, "-33.8591");
        assert_eq!(g.longitude, "151.2002");
        assert_eq!(g.postal_code, "2000");
    }

    #[test]
    fn geoip_lookup_partial_results() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "8.8.8.8", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();
        let reader = maxminddb::Reader::open_readfile("test-data/GeoLite2-City.mmdb").unwrap();

        let mut augment = Geoip::new(reader, Atom::from("remote_addr"), "geo".to_string());
        let new_event = augment.transform(event).unwrap();

        let geodata_k = Atom::from("geo".to_string());
        let geodata = new_event.as_log().get(&geodata_k);

        let geodata_s = geodata.unwrap().to_string_lossy();
        let g: GeoipDecodedData = serde_json::from_str(&geodata_s).unwrap();

        assert_eq!(g.city_name, "");
        assert_eq!(g.country_code, "US");
        assert_eq!(g.continent_code, "NA");
        assert_eq!(g.time_zone, "America/Chicago");
        assert_eq!(g.latitude, "37.751");
        assert_eq!(g.longitude, "-97.822");
        assert_eq!(g.postal_code, "");
    }
    #[test]
    fn geoip_lookup_no_results() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "10.1.12.1", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();
        let reader = maxminddb::Reader::open_readfile("test-data/GeoLite2-City.mmdb").unwrap();

        let mut augment = Geoip::new(reader, Atom::from("remote_addr"), "geo".to_string());
        let new_event = augment.transform(event).unwrap();

        let geodata_k = Atom::from("geo".to_string());
        let geodata = new_event.as_log().get(&geodata_k);

        let geodata_s = geodata.unwrap().to_string_lossy();
        let g: GeoipDecodedData = serde_json::from_str(&geodata_s).unwrap();

        assert_eq!(g.city_name, "");
        assert_eq!(g.country_code, "");
        assert_eq!(g.continent_code, "");
        assert_eq!(g.time_zone, "");
        assert_eq!(g.latitude, "");
        assert_eq!(g.longitude, "");
        assert_eq!(g.postal_code, "");
    }
}
