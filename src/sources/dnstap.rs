use super::util::build_framestream_unix_source;
use crate::{
    event::{self, Event},
    shutdown::ShutdownSignal,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use bytes::Bytes;
use futures01::sync::mpsc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod parser;

use parser::{schema::DnstapEventSchema, DnstapParser};

#[derive(Deserialize, Serialize, Debug)]
pub struct DnstapConfig {
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    pub host_key: Option<String>,
    pub path: PathBuf,
}

fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

impl DnstapConfig {
    pub fn new(path: PathBuf) -> Self {
        Self {
            host_key: None,
            max_length: default_max_length(),
            path,
        }
    }

    fn content_type(&self) -> String {
        "protobuf:dnstap.Dnstap".to_string() //content-type for framestream
    }
}

inventory::submit! {
    SourceDescription::new_without_default::<DnstapConfig>("dnstap")
}

#[typetag::serde(name = "dnstap")]
impl SourceConfig for DnstapConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        let host_key = self
            .host_key
            .clone()
            .unwrap_or(event::log_schema().host_key().to_string());
        Ok(build_framestream_unix_source(
            self.path.clone(),
            self.max_length,
            host_key,
            self.content_type(),
            shutdown,
            out,
            handle_event,
        ))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "dnstap"
    }
}

/**
 * Function to pass into util::framestream::build_framestream_unix_source
 * Takes a data frame from the unix socket and turns it into a Vector Event.
 **/
fn handle_event(host_key: &str, received_from: Option<Bytes>, frame: Bytes) -> Option<Event> {
    let mut event = Event::new_empty_log();

    let log_event = event.as_mut_log();
    log_event.insert(event::log_schema().source_type_key(), "dnstap");

    if let Some(host) = received_from {
        log_event.insert(host_key, host);
    }

    match DnstapParser::new(DnstapEventSchema::new(), log_event).parse_dnstap_data(frame) {
        Err(error) => {
            error!("Dnstap protobuf decode error {:?}", error);
            None
        }
        Ok(_) => Some(event),
    }
}
