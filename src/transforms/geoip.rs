extern crate maxminddb;

use super::Transform;

use crate::{
    event::Event,
    topology::config::{DataType, TransformConfig},
};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

use std::net::IpAddr;
use std::str::FromStr;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct GeoipConfig {
    pub source: Atom,
    pub database: String,
    pub target: String,
}

pub struct Geoip {
    pub source: Atom,
    pub database: String,
    pub target: String,
}

#[typetag::serde(name = "geoip")]
impl TransformConfig for GeoipConfig {
    fn build(&self) -> Result<Box<dyn Transform>, crate::Error> {
        Ok(Box::new(Geoip::new(
            self.source.clone(),
            self.database.clone(),
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
    pub fn new(source: Atom, database: String, target: String) -> Self {
        Geoip {
            source: source,
            database: database,
            target: target,
        }
    }
}

impl Transform for Geoip {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        println!("Event: {:?}", event.as_log());
        let ipaddress = event
            .as_log()
            .get(&self.source)
            .map(|s| s.to_string_lossy());
        if let Some(ipaddress) = &ipaddress {
            let ip: IpAddr = FromStr::from_str(ipaddress).unwrap();
            println!("Looking up {}", ip);
            let reader =
                maxminddb::Reader::open_readfile("/usr/local/share/GeoIP/GeoIP2-City.mmdb")
                    .unwrap();
            let city: maxminddb::geoip2::City = reader.lookup(ip).unwrap();
            let iso_code = city.country.and_then(|cy| cy.iso_code);
            if let Some(iso_code) = iso_code {
                event
                    .as_mut_log()
                    .insert_explicit(Atom::from("city"), iso_code.into());
            }
        } else {
            println!("Something went wrong: {:?}", Some(ipaddress));
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
    use crate::{event::Event, transforms::Transform, transforms::json_parser::{JsonParser, JsonParserConfig},};
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn geoip_event() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "8.8.8.8", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();

        let mut augment = Geoip::new(
            Atom::from("remote_addr"),
            "path/to/db".to_string(),
            "geoip".to_string(),
        );

        let new_event = augment.transform(event).unwrap();

        let key = Atom::from("city".to_string());
        let kv = new_event.as_log().get(&key);

        let val = "bar".to_string();
        assert_eq!(kv, Some(&val.into()));
    }
}
