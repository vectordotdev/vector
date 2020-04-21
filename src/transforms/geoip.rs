use super::Transform;

use crate::{
    event::{Event, Value},
    topology::config::{DataType, TransformConfig, TransformContext},
};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

use std::str::FromStr;
use tracing::field;

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

fn default_geoip_target_field() -> String {
    "geoip".to_string()
}

#[typetag::serde(name = "geoip")]
impl TransformConfig for GeoipConfig {
    fn build(&self, _cx: TransformContext) -> Result<Box<dyn Transform>, crate::Error> {
        let reader = maxminddb::Reader::open_readfile(self.database.clone())?;
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

    fn transform_type(&self) -> &'static str {
        "geoip"
    }
}

impl Geoip {
    pub fn new(dbreader: maxminddb::Reader<Vec<u8>>, source: Atom, target: String) -> Self {
        Geoip {
            dbreader,
            source,
            target,
        }
    }
}

impl Transform for Geoip {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let target_field = self.target.clone();
        let ipaddress = event
            .as_log()
            .get(&self.source)
            .map(|s| s.to_string_lossy());
        if let Some(ipaddress) = &ipaddress {
            if let Ok(ip) = FromStr::from_str(ipaddress) {
                if let Ok(data) = self.dbreader.lookup::<maxminddb::geoip2::City>(ip) {
                    if let Some(city_names) = data.city.and_then(|c| c.names) {
                        if let Some(city_name_en) = city_names.get("en") {
                            event.as_mut_log().insert(
                                Atom::from(format!("{}.city_name", target_field)),
                                Value::from(city_name_en.to_string()),
                            );
                        }
                    }

                    let continent_code = data.continent.and_then(|c| c.code);
                    if let Some(continent_code) = continent_code {
                        event.as_mut_log().insert(
                            Atom::from(format!("{}.continent_code", target_field)),
                            Value::from(continent_code),
                        );
                    }

                    let iso_code = data.country.and_then(|cy| cy.iso_code);
                    if let Some(iso_code) = iso_code {
                        event.as_mut_log().insert(
                            Atom::from(format!("{}.country_code", target_field)),
                            Value::from(iso_code),
                        );
                    }

                    let time_zone = data.location.clone().and_then(|loc| loc.time_zone);
                    if let Some(time_zone) = time_zone {
                        event.as_mut_log().insert(
                            Atom::from(format!("{}.timezone", target_field)),
                            Value::from(time_zone),
                        );
                    }

                    let latitude = data.location.clone().and_then(|loc| loc.latitude);
                    if let Some(latitude) = latitude {
                        event.as_mut_log().insert(
                            Atom::from(format!("{}.latitude", target_field)),
                            Value::from(latitude.to_string()),
                        );
                    }

                    let longitude = data.location.clone().and_then(|loc| loc.longitude);
                    if let Some(longitude) = longitude {
                        event.as_mut_log().insert(
                            Atom::from(format!("{}.longitude", target_field)),
                            Value::from(longitude.to_string()),
                        );
                    }

                    let postal_code = data.postal.clone().and_then(|p| p.code);
                    if let Some(postal_code) = postal_code {
                        event.as_mut_log().insert(
                            Atom::from(format!("{}.postal_code", target_field)),
                            Value::from(postal_code),
                        );
                    }
                }
            } else {
                debug!(
                    message = "IP Address not parsed correctly.",
                    ipaddr = &field::display(&ipaddress),
                );
            }
        } else {
            debug!(
                message = "Field does not exist.",
                field = self.source.as_ref(),
            );
        };

        // If we have any of the geoip fields missing, we insert
        // empty values so that we know that the transform was executed
        // but the lookup didn't find the result
        let geoip_fields = [
            format!("{}.city_name", target_field),
            format!("{}.country_code", target_field),
            format!("{}.continent_code", target_field),
            format!("{}.timezone", target_field),
            format!("{}.latitude", target_field),
            format!("{}.longitude", target_field),
            format!("{}.postal_code", target_field),
        ];
        for field in geoip_fields.iter() {
            let e = event.as_mut_log();
            let d = e.get(&Atom::from(field.to_string()));
            match d {
                None => {
                    e.insert(Atom::from(field.to_string()), Value::from(""));
                }
                _ => (),
            }
        }

        Some(event)
    }
}

#[cfg(feature = "transforms-json_parser")]
#[cfg(test)]
mod tests {
    use super::Geoip;
    use crate::{
        event::Event,
        transforms::json_parser::{JsonParser, JsonParserConfig},
        transforms::Transform,
    };
    use std::collections::HashMap;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn geoip_lookup_success() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "2.125.160.216", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();
        let reader = maxminddb::Reader::open_readfile("tests/data/GeoIP2-City-Test.mmdb").unwrap();

        let mut augment = Geoip::new(reader, Atom::from("remote_addr"), "geo".to_string());
        let new_event = augment.transform(event).unwrap();

        let mut exp_geoip_attr = HashMap::new();
        exp_geoip_attr.insert("city_name", "Boxford");
        exp_geoip_attr.insert("country_code", "GB");
        exp_geoip_attr.insert("continent_code", "EU");
        exp_geoip_attr.insert("timezone", "Europe/London");
        exp_geoip_attr.insert("latitude", "51.75");
        exp_geoip_attr.insert("longitude", "-1.25");
        exp_geoip_attr.insert("postal_code", "OX1");

        for field in exp_geoip_attr.keys() {
            let k = Atom::from(format!("geo.{}", field).to_string());
            let geodata = new_event.as_log().get(&k).unwrap().to_string_lossy();

            match exp_geoip_attr.get(field) {
                Some(&v) => assert_eq!(geodata, v),
                _ => assert!(false),
            }
        }
    }

    #[test]
    fn geoip_lookup_partial_results() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "67.43.156.9", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();
        let reader = maxminddb::Reader::open_readfile("tests/data/GeoIP2-City-Test.mmdb").unwrap();

        let mut augment = Geoip::new(reader, Atom::from("remote_addr"), "geo".to_string());
        let new_event = augment.transform(event).unwrap();

        let mut exp_geoip_attr = HashMap::new();
        exp_geoip_attr.insert("city_name", "");
        exp_geoip_attr.insert("country_code", "BT");
        exp_geoip_attr.insert("continent_code", "AS");
        exp_geoip_attr.insert("timezone", "Asia/Thimphu");
        exp_geoip_attr.insert("latitude", "27.5");
        exp_geoip_attr.insert("longitude", "90.5");
        exp_geoip_attr.insert("postal_code", "");

        for field in exp_geoip_attr.keys() {
            let k = Atom::from(format!("geo.{}", field).to_string());
            let geodata = new_event.as_log().get(&k).unwrap().to_string_lossy();
            match exp_geoip_attr.get(field) {
                Some(&v) => assert_eq!(geodata, v),
                _ => assert!(false),
            }
        }
    }

    #[test]
    fn geoip_lookup_no_results() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "10.1.12.1", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();
        let reader = maxminddb::Reader::open_readfile("tests/data/GeoIP2-City-Test.mmdb").unwrap();

        let mut augment = Geoip::new(reader, Atom::from("remote_addr"), "geo".to_string());
        let new_event = augment.transform(event).unwrap();

        let mut exp_geoip_attr = HashMap::new();
        exp_geoip_attr.insert("city_name", "");
        exp_geoip_attr.insert("country_code", "");
        exp_geoip_attr.insert("continent_code", "");
        exp_geoip_attr.insert("timezone", "");
        exp_geoip_attr.insert("latitude", "");
        exp_geoip_attr.insert("longitude", "");
        exp_geoip_attr.insert("postal_code", "");

        for field in exp_geoip_attr.keys() {
            let k = Atom::from(format!("geo.{}", field).to_string());
            println!("Looking for {:?}", k);
            let geodata = new_event.as_log().get(&k).unwrap().to_string_lossy();
            match exp_geoip_attr.get(field) {
                Some(&v) => assert_eq!(geodata, v),
                _ => assert!(false),
            }
        }
    }
}
