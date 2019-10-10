extern crate maxminddb;

use super::Transform;
use crate::{
    event::{Event, ValueKind},
    topology::config::{DataType, TransformConfig},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;
use toml::value::Value;

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
        let ipaddress = event
            .as_log()
            .get(&self.source)
            .map(|s| s.to_string_lossy());
        if let Some(ipaddress) = &ipaddress {
            let reader =
                maxminddb::Reader::open_readfile("/usr/local/share/GeoIP/GeoIP2-City.mmdb")
                    .unwrap();
            let ip: IpAddr = FromStr::from_str(ipaddress).unwrap();
            let city: maxminddb::geoip2::City = reader.lookup(ip).unwrap();
            let iso_code = city.country.and_then(|cy| cy.iso_code);
            if let Some(iso_code) = iso_code {
                event
                    .as_mut_log()
                    .insert_explicit(Atom::from("city"), iso_code.into());
            }
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
    use crate::{event::Event, transforms::Transform};
    use indexmap::IndexMap;
    use std::collections::HashMap;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn geoip_event() {
        let event = Event::from("augment me");
        let mut augment = Geoip::new(
            Atom::from("source"),
            "path/to/db".to_string(),
            "geoip".to_string(),
        );

        let new_event = augment.transform(event).unwrap();

        let key = Atom::from("foo".to_string());
        let kv = new_event.as_log().get(&key);

        let val = "bar".to_string();
        assert_eq!(kv, Some(&val.into()));
    }
}
