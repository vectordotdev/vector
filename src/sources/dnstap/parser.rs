use crate::{
    event::{LogEvent, PathComponent, PathIter},
    Error, Result,
};
use bytes::Bytes;
use prost::Message;
use std::convert::TryInto;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
#[cfg(unix)]
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

use proto::{Dnstap, dnstap::Type as DnstapDataType, Message as DnstapMessage};

pub fn parse_dnstap_data(log_event: &mut LogEvent, frame: Bytes) -> Result<()> {
    //parse frame with dnstap protobuf
    let proto_msg = Dnstap::decode(frame.clone())?;

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

    let dnstap_data_type: i32 = proto_msg.r#type;
    // the raw value is reserved intentionally to ensure forward-compatibility
    let mut need_raw_data = false;
    log_event.insert("type", dnstap_data_type);
    if dnstap_data_type == DnstapDataType::Message as i32 {
        //TODO: parse parts of dnstap that are left as bytes
        if let Some(message) = proto_msg.message {
            if let Err(err) = parse_dnstap_message(log_event, "data", message) {
                error!(target: "dnstap event", "failed to parse dnstap message: {}", err.to_string());
                need_raw_data = true;
                log_event.insert("error", err.to_string());
            }
        }
    } else {
        need_raw_data = true;
    }

    if need_raw_data {
        log_event.insert("data.raw_data", format_bytes_as_hex_string(&frame.to_vec()));
    }

    Ok(())
}

fn parse_dnstap_message(
    log_event: &mut LogEvent,
    key_prefix: &str,
    dnstap_message: DnstapMessage,
) -> Result<()> {
    if let Some(socket_family) = dnstap_message.socket_family {
        // the raw value is reserved intentionally to ensure forward-compatibility
        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "socket_family"),
            socket_family,
        );

        if let Some(query_address) = dnstap_message.query_address {
            let source_address = if socket_family == 1 {
                let address_buffer: [u8; 4] = query_address[0..4].try_into()?;
                IpAddr::V4(Ipv4Addr::from(address_buffer))
            } else {
                let address_buffer: [u8; 16] = query_address[0..16].try_into()?;
                IpAddr::V6(Ipv6Addr::from(address_buffer))
            };

            log_event.insert_path(
                make_event_key_with_prefix(key_prefix, "query_address"),
                source_address.to_string(),
            );
        }

        if let Some(query_port) = dnstap_message.query_port {
            log_event.insert_path(
                make_event_key_with_prefix(key_prefix, "query_port"),
                query_port as i64,
            );
        }

        if let Some(response_address) = dnstap_message.response_address {
            let response_addr = if socket_family == 1 {
                let address_buffer: [u8; 4] = response_address[0..4].try_into()?;
                IpAddr::V4(Ipv4Addr::from(address_buffer))
            } else {
                let address_buffer: [u8; 16] = response_address[0..16].try_into()?;
                IpAddr::V6(Ipv6Addr::from(address_buffer))
            };

            log_event.insert_path(
                make_event_key_with_prefix(key_prefix, "response_address"),
                response_addr.to_string(),
            );
        }

        if let Some(response_port) = dnstap_message.response_port {
            log_event.insert_path(
                make_event_key_with_prefix(key_prefix, "response_port"),
                response_port as i64,
            );
        }
    }

    if let Some(query_zone) = dnstap_message.query_zone {
        let mut decoder: BinDecoder = BinDecoder::new(&query_zone);
        match Name::read(&mut decoder) {
            Ok(raw_data) => {
                log_event.insert_path(
                    make_event_key_with_prefix(key_prefix, "query_zone"),
                    raw_data.to_utf8(),
                );
            }
            Err(error) => return Err(Error::from(error.to_string())),
        }
    }

    if let Some(query_time_sec) = dnstap_message.query_time_sec {
        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "query_time_sec"),
            query_time_sec as i64,
        );
    }

    if let Some(query_time_nsec) = dnstap_message.query_time_nsec {
        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "query_time_nsec"),
            query_time_nsec as i64,
        );
    }

    if let Some(response_time_sec) = dnstap_message.response_time_sec {
        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "response_time_sec"),
            response_time_sec as i64,
        );
    }

    if let Some(response_time_nsec) = dnstap_message.response_time_nsec {
        log_event.insert_path(
            make_event_key_with_prefix(key_prefix, "response_time_nsec"),
            response_time_nsec as i64,
        );
    }

    // the raw value is reserved intentionally to ensure forward-compatibility
    let dnstap_message_type = dnstap_message.r#type;
    log_event.insert_path(
        make_event_key_with_prefix(key_prefix, "type"),
        dnstap_message_type as i64,
    );

    match dnstap_message_type {
        1..=14 => {
            if let Some(query_message) = dnstap_message.query_message {
                if let Err(error) = parse_dns_query_message(
                    log_event,
                    &concat_event_key_paths(key_prefix, "query_message"),
                    &query_message,
                ) {
                    log_raw_dns_message(
                        log_event,
                        &concat_event_key_paths(key_prefix, "query_message"),
                        &query_message,
                    );

                    return Err(error);
                };
            }

            if let Some(response_message) = dnstap_message.response_message {
                if let Err(error) = parse_dns_query_message(
                    log_event,
                    &concat_event_key_paths(key_prefix, "response_message"),
                    &response_message,
                ) {
                    log_raw_dns_message(
                        log_event,
                        &concat_event_key_paths(key_prefix, "response_message"),
                        &response_message,
                    );

                    return Err(error);
                };
            }
        }
        _ => {
            if let Some(query_message) = dnstap_message.query_message {
                log_raw_dns_message(
                    log_event,
                    &concat_event_key_paths(key_prefix, "query_message"),
                    &query_message,
                );
            }

            if let Some(response_message) = dnstap_message.response_message {
                log_raw_dns_message(
                    log_event,
                    &concat_event_key_paths(key_prefix, "response_message"),
                    &response_message,
                );
            }
        }
    }

    Ok(())
}

fn log_raw_dns_message(log_event: &mut LogEvent, key_prefix: &str, raw_dns_message: &Vec<u8>) {
    log_event.insert_path(
        make_event_key_with_prefix(key_prefix, "raw_data"),
        format_bytes_as_hex_string(raw_dns_message),
    );
}

fn parse_dns_query_message(
    log_event: &mut LogEvent,
    key_prefix: &str,
    raw_dns_message: &Vec<u8>,
) -> Result<()> {
    if let Ok(msg) = TrustDnsMessage::from_vec(raw_dns_message) {
        println!("Query: {:?}", msg);

        parse_dns_query_message_header(
            log_event,
            &concat_event_key_paths(key_prefix, "header"),
            &msg,
        );

        parse_dns_query_message_query_section(
            log_event,
            &concat_event_key_paths(key_prefix, "question"),
            &msg,
        )?;

        parse_dns_query_message_answer_section(
            log_event,
            &concat_event_key_paths(key_prefix, "answer"),
            &msg,
        )?;

        parse_dns_query_message_authority_section(
            log_event,
            &concat_event_key_paths(key_prefix, "authority"),
            &msg,
        )?;

        parse_dns_query_message_additional_section(
            log_event,
            &concat_event_key_paths(key_prefix, "additional"),
            &msg,
        )?;

        parse_edns(log_event, &concat_event_key_paths(key_prefix, "opt"), &msg);
    };

    Ok(())
}

fn parse_dns_query_message_header(
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

fn parse_dns_query_message_query_section(
    log_event: &mut LogEvent,
    key_path: &str,
    dns_message: &TrustDnsMessage,
) -> Result<()> {
    for (i, query) in dns_message.queries().iter().enumerate() {
        log_event.insert_path(
            make_event_key_with_index(key_path, i as u32),
            query.to_string(),
        );
    }

    Ok(())
}

fn parse_dns_query_message_answer_section(
    log_event: &mut LogEvent,
    key_path: &str,
    dns_message: &TrustDnsMessage,
) -> Result<()> {
    for (i, record) in dns_message.answers().iter().enumerate() {
        parse_dns_record(
            log_event,
            &make_indexed_event_key_path(key_path, i as u32),
            record,
        )?;
    }

    Ok(())
}

fn parse_dns_query_message_authority_section(
    log_event: &mut LogEvent,
    key_path: &str,
    dns_message: &TrustDnsMessage,
) -> Result<()> {
    for (i, record) in dns_message.name_servers().iter().enumerate() {
        parse_dns_record(
            log_event,
            &make_indexed_event_key_path(key_path, i as u32),
            record,
        )?;
    }

    Ok(())
}

fn parse_dns_query_message_additional_section(
    log_event: &mut LogEvent,
    key_path: &str,
    dns_message: &TrustDnsMessage,
) -> Result<()> {
    for (i, record) in dns_message.additionals().iter().enumerate() {
        parse_dns_record(
            log_event,
            &make_indexed_event_key_path(key_path, i as u32),
            record,
        )?;
    }

    Ok(())
}

fn parse_edns(log_event: &mut LogEvent, key_prefix: &str, dns_message: &TrustDnsMessage) {
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
        parse_edns_options(
            log_event,
            &concat_event_key_paths(key_prefix, "options"),
            edns,
        );
    }
}

fn parse_edns_options(log_event: &mut LogEvent, key_path: &str, edns: &Edns) {
    edns.options()
        .options()
        .iter()
        .enumerate()
        .for_each(|(i, (code, option))| {
            match option {
                EdnsOption::DAU(algorithms) => parse_edns_opt_dnssec_algorithms(
                    log_event,
                    &make_indexed_event_key_path(key_path, i as u32),
                    code,
                    algorithms,
                ),
                EdnsOption::DHU(algorithms) => parse_edns_opt_dnssec_algorithms(
                    log_event,
                    &make_indexed_event_key_path(key_path, i as u32),
                    code,
                    algorithms,
                ),
                EdnsOption::N3U(algorithms) => parse_edns_opt_dnssec_algorithms(
                    log_event,
                    &make_indexed_event_key_path(key_path, i as u32),
                    code,
                    algorithms,
                ),
                EdnsOption::Unknown(_, opt_data) => parse_edns_opt(
                    log_event,
                    &make_indexed_event_key_path(key_path, i as u32),
                    code,
                    opt_data,
                ),
            };
        });
}

fn parse_edns_opt_dnssec_algorithms(
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

fn parse_edns_opt(
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

fn parse_dns_record(log_event: &mut LogEvent, key_path: &str, record: &Record) -> Result<()> {
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
        format_rdata(record.rdata())?,
    );

    Ok(())
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

fn format_rdata(rdata: &RData) -> Result<String> {
    match rdata {
        RData::A(ip) => Ok(ip.to_string()),
        RData::AAAA(ip) => Ok(ip.to_string()),
        RData::CNAME(name) => Ok(name.to_utf8()),
        RData::SRV(srv) => Ok(format!(
            "{} {} {} {}",
            srv.priority(),
            srv.weight(),
            srv.port(),
            srv.target().to_utf8()
        )),
        RData::TXT(txt) => Ok(txt
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
            .join(" ")),
        RData::SOA(soa) => Ok(format!(
            "{} {} ({} {} {} {} {})",
            soa.mname().to_utf8(),
            soa.rname().to_utf8(),
            soa.serial(),
            soa.refresh(),
            soa.retry(),
            soa.expire(),
            soa.minimum()
        )),
        RData::Unknown { code, rdata } => match code {
            13 => match rdata.anything() {
                Some(raw_rdata) => {
                    let mut decoder = BinDecoder::new(raw_rdata);
                    let cpu = parse_character_string(&mut decoder)?;
                    let os = parse_character_string(&mut decoder)?;
                    Ok(format!(
                        "\"{}\" \"{}\"",
                        escape_string_for_text_representation(cpu),
                        escape_string_for_text_representation(os)
                    ))
                }
                None => Err(Error::from("Empty HINFO rdata")),
            },

            _ => match rdata.anything() {
                Some(raw_rdata) => Ok(format_bytes_as_hex_string(raw_rdata)),
                None => Err(Error::from("Empty rdata")),
            },
        },
        _ => Ok(String::from("unknown yet")),
    }
}

fn parse_character_string(decoder: &mut BinDecoder) -> Result<String> {
    match decoder.read_u8() {
        Ok(raw_len) => {
            let len = raw_len.unverified() as usize;
            match decoder.read_slice(len) {
                Ok(raw_text) => match raw_text.verify_unwrap(|r| r.len() == len) {
                    Ok(verified_text) => Ok(String::from_utf8_lossy(verified_text).to_string()),
                    Err(raw_data) => Err(Error::from(format!(
                        "Unexpected data length: expected {}, got {}. Raw data {}",
                        len,
                        raw_data.len(),
                        format_bytes_as_hex_string(&raw_data.to_vec())
                    ))),
                },
                Err(error) => Err(Error::from(error.to_string())),
            }
        }
        Err(error) => Err(Error::from(error.to_string())),
    }
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
