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
    op::{message::Message as TrustDnsMessage, Edns, Query},
    rr::{
        dnssec::SupportedAlgorithms,
        domain::Name,
        rdata::opt::{EdnsCode, EdnsOption},
        record_data::RData,
        resource::Record,
        RecordType,
    },
    serialize::binary::{BinDecodable, BinDecoder},
};

mod proto {
    include!(concat!(env!("OUT_DIR"), "/dnstap.rs"));
}

use proto::{dnstap::Type as DnstapDataType, Dnstap, Message as DnstapMessage};

pub mod schema;

use schema::DnstapEventSchema;

pub struct DnstapParser<'a> {
    event_schema: &'a DnstapEventSchema,
    log_event: &'a mut LogEvent,
}

impl<'a> DnstapParser<'a> {
    pub fn new(event_schema: &'a DnstapEventSchema, log_event: &'a mut LogEvent) -> Self {
        Self {
            event_schema,
            log_event,
        }
    }

    pub fn parse_dnstap_data(self: &mut Self, frame: Bytes) -> Result<()> {
        //parse frame with dnstap protobuf
        let proto_msg = Dnstap::decode(frame.clone())?;

        if let Some(server_id) = proto_msg.identity {
            self.log_event.insert(
                self.event_schema
                    .dnstap_root_data_schema()
                    .server_identity(),
                String::from_utf8(server_id).unwrap_or_default(),
            );
        }

        if let Some(version) = proto_msg.version {
            self.log_event.insert(
                self.event_schema.dnstap_root_data_schema().server_version(),
                String::from_utf8(version).unwrap_or_default(),
            );
        }

        if let Some(extra) = proto_msg.extra {
            self.log_event.insert(
                self.event_schema.dnstap_root_data_schema().extra(),
                String::from_utf8(extra).unwrap_or_default(),
            );
        }

        let dnstap_data_type: i32 = proto_msg.r#type;
        // the raw value is reserved intentionally to ensure forward-compatibility
        let mut need_raw_data = false;
        self.log_event.insert(
            self.event_schema.dnstap_root_data_schema().data_type(),
            dnstap_data_type,
        );
        if dnstap_data_type == DnstapDataType::Message as i32 {
            //TODO: parse parts of dnstap that are left as bytes
            if let Some(message) = proto_msg.message {
                if let Err(err) = self.parse_dnstap_message(message) {
                    error!(target: "dnstap event", "failed to parse dnstap message: {}", err.to_string());
                    need_raw_data = true;
                    self.log_event.insert(
                        self.event_schema.dnstap_root_data_schema().error(),
                        err.to_string(),
                    );
                }
            }
        } else {
            need_raw_data = true;
        }

        if need_raw_data {
            self.log_event.insert(
                self.event_schema.dnstap_root_data_schema().raw_data(),
                format_bytes_as_hex_string(&frame.to_vec()),
            );
        }

        Ok(())
    }

    fn parse_dnstap_message(self: &mut Self, dnstap_message: DnstapMessage) -> Result<()> {
        if let Some(socket_family) = dnstap_message.socket_family {
            // the raw value is reserved intentionally to ensure forward-compatibility
            self.log_event.insert(
                self.event_schema.dnstap_message_schema().socket_family(),
                socket_family,
            );

            if let Some(socket_protocol) = dnstap_message.socket_protocol {
                // the raw value is reserved intentionally to ensure forward-compatibility
                self.log_event.insert(
                    self.event_schema.dnstap_message_schema().socket_protocol(),
                    socket_protocol,
                );
            }

            if let Some(query_address) = dnstap_message.query_address {
                let source_address = if socket_family == 1 {
                    let address_buffer: [u8; 4] = query_address[0..4].try_into()?;
                    IpAddr::V4(Ipv4Addr::from(address_buffer))
                } else {
                    let address_buffer: [u8; 16] = query_address[0..16].try_into()?;
                    IpAddr::V6(Ipv6Addr::from(address_buffer))
                };

                self.log_event.insert(
                    self.event_schema.dnstap_message_schema().query_address(),
                    source_address.to_string(),
                );
            }

            if let Some(query_port) = dnstap_message.query_port {
                self.log_event.insert(
                    self.event_schema.dnstap_message_schema().query_port(),
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

                self.log_event.insert(
                    self.event_schema.dnstap_message_schema().response_address(),
                    response_addr.to_string(),
                );
            }

            if let Some(response_port) = dnstap_message.response_port {
                self.log_event.insert(
                    self.event_schema.dnstap_message_schema().response_port(),
                    response_port as i64,
                );
            }
        }

        if let Some(query_zone) = dnstap_message.query_zone {
            let mut decoder: BinDecoder = BinDecoder::new(&query_zone);
            match Name::read(&mut decoder) {
                Ok(raw_data) => {
                    self.log_event.insert(
                        self.event_schema.dnstap_message_schema().query_zone(),
                        raw_data.to_utf8(),
                    );
                }
                Err(error) => return Err(Error::from(error.to_string())),
            }
        }

        if let Some(query_time_sec) = dnstap_message.query_time_sec {
            self.log_event.insert(
                self.event_schema.dnstap_message_schema().query_time_sec(),
                query_time_sec as i64,
            );
        }

        if let Some(query_time_nsec) = dnstap_message.query_time_nsec {
            self.log_event.insert(
                self.event_schema.dnstap_message_schema().query_time_nsec(),
                query_time_nsec as i64,
            );
        }

        if let Some(response_time_sec) = dnstap_message.response_time_sec {
            self.log_event.insert(
                self.event_schema
                    .dnstap_message_schema()
                    .response_time_sec(),
                response_time_sec as i64,
            );
        }

        if let Some(response_time_nsec) = dnstap_message.response_time_nsec {
            self.log_event.insert(
                self.event_schema
                    .dnstap_message_schema()
                    .response_time_nsec(),
                response_time_nsec as i64,
            );
        }

        // the raw value is reserved intentionally to ensure forward-compatibility
        let dnstap_message_type = dnstap_message.r#type;
        self.log_event.insert(
            self.event_schema
                .dnstap_message_schema()
                .dnstap_message_type(),
            dnstap_message_type as i64,
        );

        let query_message_key = self
            .event_schema
            .dnstap_message_schema()
            .query_message()
            .to_string();
        let response_message_key = self
            .event_schema
            .dnstap_message_schema()
            .response_message()
            .to_string();
        match dnstap_message_type {
            1..=14 => {
                if let Some(query_message) = dnstap_message.query_message {
                    if let Err(error) =
                        self.parse_dns_query_message(&query_message_key, &query_message)
                    {
                        self.log_raw_dns_message(&query_message_key, &query_message);

                        return Err(error);
                    };
                }

                if let Some(response_message) = dnstap_message.response_message {
                    if let Err(error) =
                        self.parse_dns_query_message(&response_message_key, &response_message)
                    {
                        self.log_raw_dns_message(&response_message_key, &response_message);

                        return Err(error);
                    };
                }
            }
            _ => {
                if let Some(query_message) = dnstap_message.query_message {
                    self.log_raw_dns_message(&query_message_key, &query_message);
                }

                if let Some(response_message) = dnstap_message.response_message {
                    self.log_raw_dns_message(&response_message_key, &response_message);
                }
            }
        }

        Ok(())
    }

    fn log_raw_dns_message(self: &mut Self, key_prefix: &str, raw_dns_message: &Vec<u8>) {
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_message_schema().raw_data(),
            ),
            format_bytes_as_hex_string(raw_dns_message),
        );
    }

    fn parse_dns_query_message(
        self: &mut Self,
        key_prefix: &str,
        raw_dns_message: &Vec<u8>,
    ) -> Result<()> {
        if let Ok(msg) = TrustDnsMessage::from_vec(raw_dns_message) {
            println!("Query: {:?}", msg);

            self.parse_dns_query_message_header(
                &concat_event_key_paths(
                    key_prefix,
                    self.event_schema.dns_query_message_schema().header(),
                ),
                &msg,
            );

            self.parse_dns_query_message_query_section(
                &concat_event_key_paths(
                    key_prefix,
                    self.event_schema
                        .dns_query_message_schema()
                        .question_section(),
                ),
                &msg,
            )?;

            self.parse_dns_query_message_answer_section(
                &concat_event_key_paths(
                    key_prefix,
                    self.event_schema
                        .dns_query_message_schema()
                        .answer_section(),
                ),
                &msg,
            )?;

            self.parse_dns_query_message_authority_section(
                &concat_event_key_paths(
                    key_prefix,
                    self.event_schema
                        .dns_query_message_schema()
                        .authority_section(),
                ),
                &msg,
            )?;

            self.parse_dns_query_message_additional_section(
                &concat_event_key_paths(
                    key_prefix,
                    self.event_schema
                        .dns_query_message_schema()
                        .additional_section(),
                ),
                &msg,
            )?;

            self.parse_edns(
                &concat_event_key_paths(
                    key_prefix,
                    self.event_schema
                        .dns_query_message_schema()
                        .opt_pseudo_section(),
                ),
                &msg,
            );
        };

        Ok(())
    }

    fn parse_dns_query_message_header(
        self: &mut Self,
        key_prefix: &str,
        dns_message: &TrustDnsMessage,
    ) {
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_header_schema().id(),
            ),
            dns_message.header().id() as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_header_schema().opcode(),
            ),
            dns_message.header().op_code() as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_header_schema().rcode(),
            ),
            dns_message.header().response_code() as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_header_schema().qr(),
            ),
            dns_message.header().message_type() as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_header_schema().aa(),
            ),
            dns_message.header().authoritative() as bool,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_header_schema().tc(),
            ),
            dns_message.header().truncated() as bool,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_header_schema().rd(),
            ),
            dns_message.header().recursion_desired() as bool,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_header_schema().ra(),
            ),
            dns_message.header().recursion_available() as bool,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_header_schema().ad(),
            ),
            dns_message.header().authentic_data() as bool,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_header_schema().cd(),
            ),
            dns_message.header().checking_disabled() as bool,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_header_schema().question_count(),
            ),
            dns_message.header().query_count() as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_query_header_schema().answer_count(),
            ),
            dns_message.header().answer_count() as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema
                    .dns_query_header_schema()
                    .authority_count(),
            ),
            dns_message.header().name_server_count() as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema
                    .dns_query_header_schema()
                    .additional_count(),
            ),
            dns_message.header().additional_count() as i64,
        );
    }

    fn parse_dns_query_message_query_section(
        self: &mut Self,
        key_path: &str,
        dns_message: &TrustDnsMessage,
    ) -> Result<()> {
        for (i, query) in dns_message.queries().iter().enumerate() {
            self.parse_dns_query_question(&make_indexed_event_key_path(key_path, i as u32), query)?;
        }

        Ok(())
    }

    fn parse_dns_query_question(self: &mut Self, key_path: &str, question: &Query) -> Result<()> {
        self.log_event.insert_path(
            make_event_key_with_prefix(key_path, self.event_schema.dns_record_schema().name()),
            question.name().to_string(),
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_path,
                self.event_schema.dns_record_schema().record_type(),
            ),
            match question.query_type() {
                RecordType::Unknown( code ) => parse_unknown_record_type(code),
                _ => question.query_type().to_string(),
            },
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(key_path, self.event_schema.dns_record_schema().class()),
            question.query_class().to_string(),
        );

        Ok(())
    }

    fn parse_dns_query_message_answer_section(
        self: &mut Self,
        key_path: &str,
        dns_message: &TrustDnsMessage,
    ) -> Result<()> {
        for (i, record) in dns_message.answers().iter().enumerate() {
            self.parse_dns_record(&make_indexed_event_key_path(key_path, i as u32), record)?;
        }

        Ok(())
    }

    fn parse_dns_query_message_authority_section(
        self: &mut Self,
        key_path: &str,
        dns_message: &TrustDnsMessage,
    ) -> Result<()> {
        for (i, record) in dns_message.name_servers().iter().enumerate() {
            self.parse_dns_record(&make_indexed_event_key_path(key_path, i as u32), record)?;
        }

        Ok(())
    }

    fn parse_dns_query_message_additional_section(
        self: &mut Self,
        key_path: &str,
        dns_message: &TrustDnsMessage,
    ) -> Result<()> {
        for (i, record) in dns_message.additionals().iter().enumerate() {
            self.parse_dns_record(&make_indexed_event_key_path(key_path, i as u32), record)?;
        }

        Ok(())
    }

    fn parse_edns(self: &mut Self, key_prefix: &str, dns_message: &TrustDnsMessage) {
        if let Some(edns) = dns_message.edns() {
            self.log_event.insert_path(
                make_event_key_with_prefix(
                    key_prefix,
                    self.event_schema
                        .dns_message_opt_pseudo_section_schema()
                        .extended_rcode(),
                ),
                edns.rcode_high() as i64,
            );
            self.log_event.insert_path(
                make_event_key_with_prefix(
                    key_prefix,
                    self.event_schema
                        .dns_message_opt_pseudo_section_schema()
                        .version(),
                ),
                edns.version() as i64,
            );
            self.log_event.insert_path(
                make_event_key_with_prefix(
                    key_prefix,
                    self.event_schema
                        .dns_message_opt_pseudo_section_schema()
                        .do_flag(),
                ),
                edns.dnssec_ok() as bool,
            );
            self.log_event.insert_path(
                make_event_key_with_prefix(
                    key_prefix,
                    self.event_schema
                        .dns_message_opt_pseudo_section_schema()
                        .udp_max_payload_size(),
                ),
                edns.max_payload() as i64,
            );
            self.parse_edns_options(
                &concat_event_key_paths(
                    key_prefix,
                    self.event_schema
                        .dns_message_opt_pseudo_section_schema()
                        .options(),
                ),
                edns,
            );
        }
    }

    fn parse_edns_options(self: &mut Self, key_path: &str, edns: &Edns) {
        edns.options()
            .options()
            .iter()
            .enumerate()
            .for_each(|(i, (code, option))| {
                match option {
                    EdnsOption::DAU(algorithms) => self.parse_edns_opt_dnssec_algorithms(
                        &make_indexed_event_key_path(key_path, i as u32),
                        code,
                        algorithms,
                    ),
                    EdnsOption::DHU(algorithms) => self.parse_edns_opt_dnssec_algorithms(
                        &make_indexed_event_key_path(key_path, i as u32),
                        code,
                        algorithms,
                    ),
                    EdnsOption::N3U(algorithms) => self.parse_edns_opt_dnssec_algorithms(
                        &make_indexed_event_key_path(key_path, i as u32),
                        code,
                        algorithms,
                    ),
                    EdnsOption::Unknown(_, opt_data) => self.parse_edns_opt(
                        &make_indexed_event_key_path(key_path, i as u32),
                        code,
                        opt_data,
                    ),
                };
            });
    }

    fn parse_edns_opt_dnssec_algorithms(
        self: &mut Self,
        key_prefix: &str,
        opt_code: &EdnsCode,
        algorithms: &SupportedAlgorithms,
    ) {
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_message_option_schema().opt_code(),
            ),
            Into::<u16>::into(*opt_code) as i64,
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_message_option_schema().opt_name(),
            ),
            format!("{:?}", opt_code),
        );
        algorithms.iter().enumerate().for_each(|(i, alg)| {
            self.log_event.insert_path(
                make_event_key_with_index(
                    &concat_event_key_paths(
                        key_prefix,
                        self.event_schema
                            .dns_message_option_schema()
                            .supported_algorithms(),
                    ),
                    i as u32,
                ),
                alg.to_string(),
            );
        });
    }

    fn parse_edns_opt(self: &mut Self, key_prefix: &str, opt_code: &EdnsCode, opt_data: &Vec<u8>) {
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_message_option_schema().opt_code(),
            ),
            Into::<u16>::into(*opt_code) as i64,
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_message_option_schema().opt_name(),
            ),
            format!("{:?}", opt_code),
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                self.event_schema.dns_message_option_schema().opt_data(),
            ),
            format_bytes_as_hex_string(&opt_data),
        );
    }

    fn parse_dns_record(self: &mut Self, key_path: &str, record: &Record) -> Result<()> {
        self.log_event.insert_path(
            make_event_key_with_prefix(key_path, self.event_schema.dns_record_schema().name()),
            record.name().to_string(),
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_path,
                self.event_schema.dns_record_schema().record_type(),
            ),
            match record.rdata() {
                RData::Unknown { code, rdata: _ } => parse_unknown_record_type(*code),
                _ => record.record_type().to_string(),
            },
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(key_path, self.event_schema.dns_record_schema().ttl()),
            record.ttl() as i64,
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(key_path, self.event_schema.dns_record_schema().class()),
            record.dns_class().to_string(),
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(key_path, self.event_schema.dns_record_schema().rdata()),
            format_rdata(record.rdata())?,
        );

        Ok(())
    }
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
