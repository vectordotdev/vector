use super::util::build_framestream_unix_source;
use crate::{
    event::{self, Event, LogEvent, PathComponent, PathIter},
    shutdown::ShutdownSignal,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use bytes::Bytes;
use futures01::sync::mpsc;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
#[cfg(unix)]
use std::path::PathBuf;
use trust_dns_proto::{
    op::{message::Message as TrustDnsMessage, Edns},
    rr::{
        dnssec::SupportedAlgorithms,
        domain::Name,
        rdata::opt::{EdnsCode, EdnsOption},
        record_data::RData,
        resource::Record,
    },
    serialize::binary::{BinDecodable, BinDecoder},
};

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

    let mut event = Event::new_empty_log();

    let log_event = event.as_mut_log();
    log_event.insert(event::log_schema().source_type_key(), "dnstap");

    if let Some(host) = received_from {
        log_event.insert(host_key, host);
    }

    if let Some(server_id) = proto_msg.identity {
        log_event.insert(
            "server_identity",
            String::from_utf8(server_id).unwrap_or_default(),
        );
    }

    if let Some(version) = proto_msg.version {
        log_event.insert(
            "server_version",
            String::from_utf8(version).unwrap_or_default(),
        );
    }

    if let Some(extra) = proto_msg.extra {
        log_event.insert("extra", String::from_utf8(extra).unwrap_or_default());
    }

    // the raw value is reserved intentionally to ensure forward-compatibility
    log_event.insert("type", proto_msg.r#type);

    //TODO: parse parts of dnstap that are left as bytes
    if let Some(message) = proto_msg.message {
        // the raw value is reserved intentionally to ensure forward-compatibility
        log_event.insert_path(make_event_key("message.type"), message.r#type as i64);

        if let Some(socket_family) = message.socket_family {
            // the raw value is reserved intentionally to ensure forward-compatibility
            log_event.insert_path(make_event_key("message.socket_family"), socket_family);

            if let Some(query_address) = message.query_address {
                let source_address = if socket_family == 1 {
                    let address_buffer: [u8; 4] = query_address[0..4].try_into().unwrap();
                    IpAddr::V4(Ipv4Addr::from(address_buffer))
                } else {
                    let address_buffer: [u8; 16] = query_address[0..16].try_into().unwrap();
                    IpAddr::V6(Ipv6Addr::from(address_buffer))
                };

                log_event.insert_path(
                    make_event_key("message.query_address"),
                    source_address.to_string(),
                );
            }

            if let Some(query_port) = message.query_port {
                log_event.insert_path(make_event_key("message.query_port"), query_port as i64);
            }

            if let Some(response_address) = message.response_address {
                let response_addr = if socket_family == 1 {
                    let address_buffer: [u8; 4] = response_address[0..4].try_into().unwrap();
                    IpAddr::V4(Ipv4Addr::from(address_buffer))
                } else {
                    let address_buffer: [u8; 16] = response_address[0..16].try_into().unwrap();
                    IpAddr::V6(Ipv6Addr::from(address_buffer))
                };

                log_event.insert_path(
                    make_event_key("message.response_address"),
                    response_addr.to_string(),
                );
            }

            if let Some(response_port) = message.response_port {
                log_event.insert_path(
                    make_event_key("message.response_port"),
                    response_port as i64,
                );
            }
        }

        if let Some(query_zone) = message.query_zone {
            let mut decoder: BinDecoder = BinDecoder::new(&query_zone);
            log_event.insert_path(
                make_event_key("message.query_zone"),
                Name::read(&mut decoder).unwrap().to_utf8(),
            );
        }

        if let Some(query_time_sec) = message.query_time_sec {
            log_event.insert_path(
                make_event_key("message.query_time_sec"),
                query_time_sec as i64,
            );
        }

        if let Some(query_time_nsec) = message.query_time_nsec {
            log_event.insert_path(
                make_event_key("message.query_time_nsec"),
                query_time_nsec as i64,
            );
        }

        if let Some(response_time_sec) = message.response_time_sec {
            log_event.insert_path(
                make_event_key("message.response_time_sec"),
                response_time_sec as i64,
            );
        }

        if let Some(response_time_nsec) = message.response_time_nsec {
            log_event.insert_path(
                make_event_key("message.response_time_nsec"),
                response_time_nsec as i64,
            );
        }

        if let Some(query_message) = message.query_message {
            decode_dns_query_message(log_event, "message.query_message", query_message);
        }
        if let Some(response_message) = message.response_message {
            decode_dns_query_message(log_event, "message.response_message", response_message);
        }
    }

    fn decode_dns_query_message(
        log_event: &mut LogEvent,
        key_prefix: &str,
        raw_dns_message: Vec<u8>,
    ) {
        if let Ok(msg) = TrustDnsMessage::from_vec(&raw_dns_message) {
            println!("Query: {:?}", msg);

            decode_dns_query_message_header(
                log_event,
                &concat_event_key_paths(key_prefix, "header"),
                &msg,
            );

            decode_dns_query_message_query_section(
                log_event,
                &concat_event_key_paths(key_prefix, "question"),
                &msg,
            );

            decode_dns_query_message_answer_section(
                log_event,
                &concat_event_key_paths(key_prefix, "answer"),
                &msg,
            );

            decode_dns_query_message_authority_section(
                log_event,
                &concat_event_key_paths(key_prefix, "authority"),
                &msg,
            );

            decode_dns_query_message_additional_section(
                log_event,
                &concat_event_key_paths(key_prefix, "additional"),
                &msg,
            );

            decode_edns(log_event, &concat_event_key_paths(key_prefix, "opt"), &msg);
        };
    }

    fn decode_dns_query_message_header(
        log_event: &mut LogEvent,
        key_prefix: &str,
        dns_message: &TrustDnsMessage,
    ) {
        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "id"),
            dns_message.header().id() as i64,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "opcode"),
            dns_message.header().op_code() as i64,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "rcode"),
            dns_message.header().response_code() as i64,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "qr"),
            dns_message.header().message_type() as i64,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "aa"),
            dns_message.header().authoritative() as bool,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "tc"),
            dns_message.header().truncated() as bool,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "rd"),
            dns_message.header().recursion_desired() as bool,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "ra"),
            dns_message.header().recursion_available() as bool,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "ad"),
            dns_message.header().authentic_data() as bool,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "cd"),
            dns_message.header().checking_disabled() as bool,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "qdcount"),
            dns_message.header().query_count() as i64,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "ancount"),
            dns_message.header().answer_count() as i64,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "nscount"),
            dns_message.header().name_server_count() as i64,
        );

        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "arcount"),
            dns_message.header().additional_count() as i64,
        );
    }

    fn decode_dns_query_message_query_section(
        log_event: &mut LogEvent,
        key_path: &str,
        dns_message: &TrustDnsMessage,
    ) {
        dns_message
            .queries()
            .iter()
            .enumerate()
            .for_each(|(i, query)| {
                log_event.insert_path(
                    make_event_key_with_index(key_path, i as u32),
                    query.to_string(),
                );
            });
    }

    fn decode_dns_query_message_answer_section(
        log_event: &mut LogEvent,
        key_path: &str,
        dns_message: &TrustDnsMessage,
    ) {
        dns_message
            .answers()
            .iter()
            .enumerate()
            .for_each(|(i, record)| {
                decode_dns_record(
                    log_event,
                    &make_indexed_event_key_path(key_path, i as u32),
                    record,
                );
            });
    }

    fn decode_dns_query_message_authority_section(
        log_event: &mut LogEvent,
        key_path: &str,
        dns_message: &TrustDnsMessage,
    ) {
        dns_message
            .name_servers()
            .iter()
            .enumerate()
            .for_each(|(i, record)| {
                decode_dns_record(
                    log_event,
                    &make_indexed_event_key_path(key_path, i as u32),
                    record,
                );
            });
    }

    fn decode_dns_query_message_additional_section(
        log_event: &mut LogEvent,
        key_path: &str,
        dns_message: &TrustDnsMessage,
    ) {
        dns_message
            .additionals()
            .iter()
            .enumerate()
            .for_each(|(i, record)| {
                decode_dns_record(
                    log_event,
                    &make_indexed_event_key_path(key_path, i as u32),
                    record,
                );
            });
    }

    fn decode_edns(log_event: &mut LogEvent, key_prefix: &str, dns_message: &TrustDnsMessage) {
        if let Some(edns) = dns_message.edns() {
            log_event.insert_path(
                make_event_key_with_prefix(key_prefix, "extended_rcode"),
                edns.rcode_high() as i64,
            );
            log_event.insert_path(
                make_event_key_with_prefix(key_prefix, "version"),
                edns.version() as i64,
            );
            log_event.insert_path(
                make_event_key_with_prefix(key_prefix, "do"),
                edns.dnssec_ok() as bool,
            );
            log_event.insert_path(
                make_event_key_with_prefix(key_prefix, "udp_max_payload_size"),
                edns.max_payload() as i64,
            );
            decode_edns_options(
                log_event,
                &concat_event_key_paths(key_prefix, "options"),
                edns,
            );
        }
    }

    fn decode_edns_options(log_event: &mut LogEvent, key_path: &str, edns: &Edns) {
        edns.options()
            .options()
            .iter()
            .enumerate()
            .for_each(|(i, (code, option))| {
                match option {
                    EdnsOption::DAU(algorithms) => decode_edns_opt_dnssec_algorithms(
                        log_event,
                        &make_indexed_event_key_path(key_path, i as u32),
                        code,
                        algorithms,
                    ),
                    EdnsOption::DHU(algorithms) => decode_edns_opt_dnssec_algorithms(
                        log_event,
                        &make_indexed_event_key_path(key_path, i as u32),
                        code,
                        algorithms,
                    ),
                    EdnsOption::N3U(algorithms) => decode_edns_opt_dnssec_algorithms(
                        log_event,
                        &make_indexed_event_key_path(key_path, i as u32),
                        code,
                        algorithms,
                    ),
                    EdnsOption::Unknown(_, opt_data) => decode_edns_opt(
                        log_event,
                        &make_indexed_event_key_path(key_path, i as u32),
                        code,
                        opt_data,
                    ),
                };
            });
    }

    fn decode_edns_opt_dnssec_algorithms(
        log_event: &mut LogEvent,
        key_prefix: &str,
        opt_code: &EdnsCode,
        algorithms: &SupportedAlgorithms,
    ) {
        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "opt_code"),
            Into::<u16>::into(*opt_code) as i64,
        );
        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "opt_name"),
            format!("{:?}", opt_code),
        );
        algorithms.iter().enumerate().for_each(|(i, alg)| {
            log_event.insert_path(
                make_event_key_with_index(
                    &concat_event_key_paths(key_prefix, "supported_algorithms"),
                    i as u32,
                ),
                alg.to_string(),
            );
        });
    }

    fn decode_edns_opt(
        log_event: &mut LogEvent,
        key_prefix: &str,
        opt_code: &EdnsCode,
        opt_data: &Vec<u8>,
    ) {
        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "opt_code"),
            Into::<u16>::into(*opt_code) as i64,
        );
        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "opt_name"),
            format!("{:?}", opt_code),
        );
        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "opt_data"),
            format_bytes_as_hex_string(&opt_data),
        );
    }

    fn decode_dns_record(log_event: &mut LogEvent, key_path: &str, record: &Record) {
        log_event.insert_path(
            make_event_key_with_prefix(key_path, "name"),
            record.name().to_string(),
        );
        log_event.insert_path(
            make_event_key_with_prefix(key_path, "type"),
            match record.rdata() {
                RData::Unknown { code, rdata: _ } => parse_unknown_record_type(*code),
                _ => record.record_type().to_string(),
            },
        );
        log_event.insert_path(
            make_event_key_with_prefix(key_path, "ttl"),
            record.ttl() as i64,
        );
        log_event.insert_path(
            make_event_key_with_prefix(key_path, "class"),
            record.dns_class().to_string(),
        );
        log_event.insert_path(
            make_event_key_with_prefix(key_path, "rdata"),
            format_rdata(record.rdata()),
        );
    }

    fn parse_unknown_record_type(rtype: u16) -> String {
        match rtype {
            13 => String::from("HINFO"),
            20 => String::from("ISDN"),
            38 => String::from("A6"),
            39 => String::from("DNAME"),
            _ => format!("[#{}]", rtype),
        }
    }

    fn format_bytes_as_hex_string(bytes: &Vec<u8>) -> String {
        bytes
            .iter()
            .map(|e| format!("{:02X}", e))
            .collect::<Vec<String>>()
            .join(".")
    }

    fn format_rdata(rdata: &RData) -> String {
        match rdata {
            RData::A(ip) => ip.to_string(),
            RData::AAAA(ip) => ip.to_string(),
            RData::CNAME(name) => name.to_utf8(),
            RData::SRV(srv) => format!(
                "{} {} {} {}",
                srv.priority(),
                srv.weight(),
                srv.port(),
                srv.target().to_utf8()
            ),
            RData::TXT(txt) => txt
                .txt_data()
                .iter()
                .map(|value| {
                    format!(
                        "\"{}\"",
                        escape_string_for_text_representation(
                            String::from_utf8_lossy(value).to_string()
                        )
                    )
                })
                .collect::<Vec<String>>()
                .join(" "),
            RData::SOA(soa) => format!(
                "{} {} ({} {} {} {} {})",
                soa.mname().to_utf8(),
                soa.rname().to_utf8(),
                soa.serial(),
                soa.refresh(),
                soa.retry(),
                soa.expire(),
                soa.minimum()
            ),
            RData::Unknown { code, rdata } => match code {
                13 => {
                    let mut decoder = BinDecoder::new(rdata.anything().unwrap());
                    let cpu = decode_character_string(&mut decoder);
                    let os = decode_character_string(&mut decoder);
                    format!(
                        "\"{}\" \"{}\"",
                        escape_string_for_text_representation(cpu),
                        escape_string_for_text_representation(os)
                    )
                }

                _ => format_bytes_as_hex_string(rdata.anything().unwrap()),
            },
            _ => String::from("unknown yet"),
        }
    }

    fn decode_character_string(decoder: &mut BinDecoder) -> String {
        let len = decoder.read_u8().unwrap().unverified() as usize;
        String::from_utf8_lossy(
            decoder
                .read_slice(len)
                .unwrap()
                .verify_unwrap(|r| r.len() == len)
                .unwrap(),
        )
        .to_string()
    }

    fn escape_string_for_text_representation(original_string: String) -> String {
        original_string.replace("\\", "\\\\").replace("\"", "\\\"")
    }

    fn make_event_key(name: &str) -> Vec<PathComponent> {
        PathIter::new(name).collect()
    }

    fn make_event_key_with_index(name: &str, index: u32) -> Vec<PathComponent> {
        make_event_key(&make_indexed_event_key_path(name, index))
    }

    fn make_event_key_with_prefix(prefix: &str, name: &str) -> Vec<PathComponent> {
        make_event_key(&concat_event_key_paths(prefix, name))
    }

    fn make_indexed_event_key_path(name: &str, index: u32) -> String {
        format!("{}[{}]", name, index)
    }

    fn concat_event_key_paths<'a>(prefix: &'a str, name: &'a str) -> String {
        [prefix, name].join(".")
    }

    Some(event)
}
