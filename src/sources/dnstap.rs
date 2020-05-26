use super::util::build_framestream_unix_source;
use crate::{
    event::{self, Event},
    shutdown::ShutdownSignal,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use bytes::Bytes;
use futures01::sync::mpsc;
use prost::Message;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::path::PathBuf;
use trust_dns_proto::op::message::Message as TrustDnsMessage;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/dnstap.rs"));
}

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
    //parse frame with dnstap protobuf
    let proto_msg = match proto::Dnstap::decode(frame) {
        Ok(msg) => msg,
        Err(e) => {
            error!("Dnstap protobuf decode error {:?}", e);
            return None;
        }
    };

    //TODO: parse parts of dnstap that are left as bytes
    if let Some(message) = proto_msg.message {
        if let Some(query_msg) = message.query_message {
            let query = match TrustDnsMessage::from_vec(&query_msg) {
                Ok(msg) => msg,
                Err(e) => {
                    error!("Trust dns parsing error {:?}", e);
                    return None;
                }
            };
            println!("Query: {:?}", query);
        }
        if let Some(response_msg) = message.response_message {
            let response = match TrustDnsMessage::from_vec(&response_msg) {
                Ok(msg) => msg,
                Err(e) => {
                    error!("Trust dns parsing error {:?}", e);
                    return None;
                }
            };
            println!("Response: {:?}", response);
        }
    }

    //TODO: decode dns info

    let mut event = Event::new_empty_log();
    event
        .as_mut_log()
        .insert(event::log_schema().source_type_key(), "dnstap");
    if let Some(host) = received_from {
        event.as_mut_log().insert(host_key, host);
    }
    Some(event)
}
