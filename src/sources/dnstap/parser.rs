extern crate base64;
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
    rr::domain::Name,
    serialize::binary::{BinDecodable, BinDecoder},
};

mod proto {
    include!(concat!(env!("OUT_DIR"), "/dnstap.rs"));
}

use proto::{dnstap::Type as DnstapDataType, Dnstap, Message as DnstapMessage};

use super::dns_message::{
    DnsRecord, EdnsOptionEntry, OptPseudoSection, QueryHeader, QueryQuestion,
};
use super::dns_message_parser::parse_dns_query_message;
use super::schema::DnstapEventSchema;

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

    pub fn parse_dnstap_data(&mut self, frame: Bytes) -> Result<()> {
        //parse frame with dnstap protobuf
        let proto_msg = Dnstap::decode(frame.clone())?;

        if let Some(server_id) = proto_msg.identity {
            self.log_event.insert(
                &self.event_schema.dnstap_root_data_schema.server_identity,
                String::from_utf8(server_id).unwrap_or_default(),
            );
        }

        if let Some(version) = proto_msg.version {
            self.log_event.insert(
                &self.event_schema.dnstap_root_data_schema.server_version,
                String::from_utf8(version).unwrap_or_default(),
            );
        }

        if let Some(extra) = proto_msg.extra {
            self.log_event.insert(
                &self.event_schema.dnstap_root_data_schema.extra,
                String::from_utf8(extra).unwrap_or_default(),
            );
        }

        let dnstap_data_type: i32 = proto_msg.r#type;
        // the raw value is reserved intentionally to ensure forward-compatibility
        let mut need_raw_data = false;
        self.log_event.insert(
            &self.event_schema.dnstap_root_data_schema.data_type,
            dnstap_data_type,
        );
        if dnstap_data_type == DnstapDataType::Message as i32 {
            if let Some(message) = proto_msg.message {
                if let Err(err) = self.parse_dnstap_message(message) {
                    error!(target: "dnstap event", "failed to parse dnstap message: {}", err.to_string());
                    need_raw_data = true;
                    self.log_event.insert(
                        &self.event_schema.dnstap_root_data_schema.error,
                        err.to_string(),
                    );
                }
            }
        } else {
            need_raw_data = true;
        }

        if need_raw_data {
            self.log_event.insert(
                &self.event_schema.dnstap_root_data_schema.raw_data,
                base64::encode(&frame.to_vec()),
            );
        }

        Ok(())
    }

    fn parse_dnstap_message(&mut self, dnstap_message: DnstapMessage) -> Result<()> {
        if let Some(socket_family) = dnstap_message.socket_family {
            // the raw value is reserved intentionally to ensure forward-compatibility
            self.log_event.insert(
                &self.event_schema.dnstap_message_schema.socket_family,
                socket_family,
            );

            if let Some(socket_protocol) = dnstap_message.socket_protocol {
                // the raw value is reserved intentionally to ensure forward-compatibility
                self.log_event.insert(
                    &self.event_schema.dnstap_message_schema.socket_protocol,
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
                    &self.event_schema.dnstap_message_schema.query_address,
                    source_address.to_string(),
                );
            }

            if let Some(query_port) = dnstap_message.query_port {
                self.log_event.insert(
                    &self.event_schema.dnstap_message_schema.query_port,
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
                    &self.event_schema.dnstap_message_schema.response_address,
                    response_addr.to_string(),
                );
            }

            if let Some(response_port) = dnstap_message.response_port {
                self.log_event.insert(
                    &self.event_schema.dnstap_message_schema.response_port,
                    response_port as i64,
                );
            }
        }

        if let Some(query_zone) = dnstap_message.query_zone {
            let mut decoder: BinDecoder = BinDecoder::new(&query_zone);
            match Name::read(&mut decoder) {
                Ok(raw_data) => {
                    self.log_event.insert(
                        &self.event_schema.dnstap_message_schema.query_zone,
                        raw_data.to_utf8(),
                    );
                }
                Err(error) => return Err(Error::from(error.to_string())),
            }
        }

        // the raw value is reserved intentionally to ensure forward-compatibility
        let dnstap_message_type = dnstap_message.r#type;
        self.log_event.insert(
            &self.event_schema.dnstap_message_schema.dnstap_message_type,
            dnstap_message_type as i64,
        );

        let query_message_key = self
            .event_schema
            .dnstap_message_schema
            .query_message
            .to_string();
        let response_message_key = self
            .event_schema
            .dnstap_message_schema
            .response_message
            .to_string();

        if let Some(query_time_sec) = dnstap_message.query_time_sec {
            let mut timestamp_nanosec: i64 = query_time_sec as i64 * 1_000_000_000 as i64;

            if let Some(query_time_nsec) = dnstap_message.query_time_nsec {
                timestamp_nanosec += query_time_nsec as i64;
            }

            if [1, 3, 5, 7, 9, 11, 13].contains(&dnstap_message_type) {
                self.log_event.insert(
                    &self.event_schema.dnstap_root_data_schema.timestamp,
                    timestamp_nanosec,
                );

                self.log_event.insert(
                    &self.event_schema.dnstap_root_data_schema.time_precision,
                    "ns",
                );
            }

            if dnstap_message.query_message != None {
                self.log_event.insert(
                    &concat_event_key_paths(
                        &query_message_key,
                        &self.event_schema.dns_query_message_schema.timestamp,
                    ),
                    timestamp_nanosec,
                );

                self.log_event.insert(
                    &concat_event_key_paths(
                        &query_message_key,
                        &self.event_schema.dns_query_message_schema.time_precision,
                    ),
                    "ns",
                );
            }
        }

        if let Some(response_time_sec) = dnstap_message.response_time_sec {
            let mut timestamp_nanosec: i64 = response_time_sec as i64 * 1_000_000_000 as i64;

            if let Some(response_time_nsec) = dnstap_message.response_time_nsec {
                timestamp_nanosec += response_time_nsec as i64;
            }

            if [2, 4, 6, 8, 10, 12, 14].contains(&dnstap_message_type) {
                self.log_event.insert(
                    &self.event_schema.dnstap_root_data_schema.timestamp,
                    timestamp_nanosec,
                );

                self.log_event.insert(
                    &self.event_schema.dnstap_root_data_schema.time_precision,
                    "ns",
                );
            }

            if dnstap_message.response_message != None {
                self.log_event.insert(
                    &concat_event_key_paths(
                        &response_message_key,
                        &self.event_schema.dns_query_message_schema.timestamp,
                    ),
                    timestamp_nanosec,
                );

                self.log_event.insert(
                    &concat_event_key_paths(
                        &response_message_key,
                        &self.event_schema.dns_query_message_schema.time_precision,
                    ),
                    "ns",
                );
            }
        }

        match dnstap_message_type {
            1..=12 => {
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

    fn log_raw_dns_message(&mut self, key_prefix: &str, raw_dns_message: &Vec<u8>) {
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                &self.event_schema.dns_query_message_schema.raw_data,
            ),
            base64::encode(raw_dns_message),
        );
    }

    fn parse_dns_query_message(
        &mut self,
        key_prefix: &str,
        raw_dns_message: &Vec<u8>,
    ) -> Result<()> {
        if let Ok(msg) = parse_dns_query_message(raw_dns_message) {
            // println!("Query: {:?}", msg);

            self.log_event.insert_path(
                make_event_key_with_prefix(
                    key_prefix,
                    &self.event_schema.dns_query_message_schema.response_code,
                ),
                msg.response_code as i64,
            );

            if let Some(response) = msg.response {
                self.log_event.insert_path(
                    make_event_key_with_prefix(
                        key_prefix,
                        &self.event_schema.dns_query_message_schema.response,
                    ),
                    response,
                );
            }

            self.log_dns_query_message_header(
                &concat_event_key_paths(
                    key_prefix,
                    &self.event_schema.dns_query_message_schema.header,
                ),
                &msg.header,
            );

            self.log_dns_query_message_query_section(
                &concat_event_key_paths(
                    key_prefix,
                    &self.event_schema.dns_query_message_schema.question_section,
                ),
                &msg.question_section,
            );

            self.log_dns_query_message_record_section(
                &concat_event_key_paths(
                    key_prefix,
                    &self.event_schema.dns_query_message_schema.answer_section,
                ),
                &msg.answer_section,
            );

            self.log_dns_query_message_record_section(
                &concat_event_key_paths(
                    key_prefix,
                    &self.event_schema.dns_query_message_schema.authority_section,
                ),
                &msg.authority_section,
            );

            self.log_dns_query_message_record_section(
                &concat_event_key_paths(
                    key_prefix,
                    &self
                        .event_schema
                        .dns_query_message_schema
                        .additional_section,
                ),
                &msg.additional_section,
            );

            self.log_edns(
                &concat_event_key_paths(
                    key_prefix,
                    &self
                        .event_schema
                        .dns_query_message_schema
                        .opt_pseudo_section,
                ),
                &msg.opt_pserdo_section,
            );
        };

        Ok(())
    }

    fn log_dns_query_message_header(&mut self, key_prefix: &str, header: &QueryHeader) {
        self.log_event.insert_path(
            make_event_key_with_prefix(key_prefix, &self.event_schema.dns_query_header_schema.id),
            header.id as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                &self.event_schema.dns_query_header_schema.opcode,
            ),
            header.opcode as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                &self.event_schema.dns_query_header_schema.rcode,
            ),
            header.rcode as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(key_prefix, &self.event_schema.dns_query_header_schema.qr),
            header.qr as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(key_prefix, &self.event_schema.dns_query_header_schema.aa),
            header.aa as bool,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(key_prefix, &self.event_schema.dns_query_header_schema.tc),
            header.tc as bool,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(key_prefix, &self.event_schema.dns_query_header_schema.rd),
            header.rd as bool,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(key_prefix, &self.event_schema.dns_query_header_schema.ra),
            header.ra as bool,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(key_prefix, &self.event_schema.dns_query_header_schema.ad),
            header.ad as bool,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(key_prefix, &self.event_schema.dns_query_header_schema.cd),
            header.cd as bool,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                &self.event_schema.dns_query_header_schema.question_count,
            ),
            header.question_count as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                &self.event_schema.dns_query_header_schema.answer_count,
            ),
            header.answer_count as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                &self.event_schema.dns_query_header_schema.authority_count,
            ),
            header.authority_count as i64,
        );

        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                &self.event_schema.dns_query_header_schema.additional_count,
            ),
            header.additional_count as i64,
        );
    }

    fn log_dns_query_message_query_section(
        &mut self,
        key_path: &str,
        questions: &Vec<QueryQuestion>,
    ) {
        for (i, query) in questions.iter().enumerate() {
            self.log_dns_query_question(&make_indexed_event_key_path(key_path, i as u32), query);
        }
    }

    fn log_dns_query_question(&mut self, key_path: &str, question: &QueryQuestion) {
        self.log_event.insert_path(
            make_event_key_with_prefix(key_path, &self.event_schema.dns_record_schema.name),
            question.name.clone(),
        );
        if let Some(record_type) = question.record_type.clone() {
            self.log_event.insert_path(
                make_event_key_with_prefix(
                    key_path,
                    &self.event_schema.dns_record_schema.record_type,
                ),
                record_type,
            );
        }
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_path,
                &self.event_schema.dns_record_schema.record_type_id,
            ),
            question.record_type_id as i64,
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(key_path, &self.event_schema.dns_record_schema.class),
            question.class.clone(),
        );
    }

    fn log_dns_query_message_record_section(&mut self, key_path: &str, records: &Vec<DnsRecord>) {
        for (i, record) in records.iter().enumerate() {
            self.log_dns_record(&make_indexed_event_key_path(key_path, i as u32), record);
        }
    }

    fn log_edns(&mut self, key_prefix: &str, opt_section: &Option<OptPseudoSection>) {
        if let Some(edns) = opt_section {
            self.log_event.insert_path(
                make_event_key_with_prefix(
                    key_prefix,
                    &self
                        .event_schema
                        .dns_message_opt_pseudo_section_schema
                        .extended_rcode,
                ),
                edns.extended_rcode as i64,
            );
            self.log_event.insert_path(
                make_event_key_with_prefix(
                    key_prefix,
                    &self
                        .event_schema
                        .dns_message_opt_pseudo_section_schema
                        .version,
                ),
                edns.version as i64,
            );
            self.log_event.insert_path(
                make_event_key_with_prefix(
                    key_prefix,
                    &self
                        .event_schema
                        .dns_message_opt_pseudo_section_schema
                        .do_flag,
                ),
                edns.dnssec_ok as bool,
            );
            self.log_event.insert_path(
                make_event_key_with_prefix(
                    key_prefix,
                    &self
                        .event_schema
                        .dns_message_opt_pseudo_section_schema
                        .udp_max_payload_size,
                ),
                edns.udp_max_payload_size as i64,
            );
            self.log_edns_options(
                &concat_event_key_paths(
                    key_prefix,
                    &self
                        .event_schema
                        .dns_message_opt_pseudo_section_schema
                        .options,
                ),
                &edns.options,
            );
        }
    }

    fn log_edns_options(&mut self, key_path: &str, options: &Vec<EdnsOptionEntry>) {
        options.iter().enumerate().for_each(|(i, opt)| {
            self.log_edns_opt(&make_indexed_event_key_path(key_path, i as u32), opt);
        });
    }

    fn log_edns_opt(&mut self, key_prefix: &str, opt: &EdnsOptionEntry) {
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                &self.event_schema.dns_message_option_schema.opt_code,
            ),
            opt.opt_code as i64,
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                &self.event_schema.dns_message_option_schema.opt_name,
            ),
            opt.opt_name.clone(),
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_prefix,
                &self.event_schema.dns_message_option_schema.opt_data,
            ),
            opt.opt_data.clone(),
        );
    }

    fn log_dns_record(&mut self, key_path: &str, record: &DnsRecord) {
        self.log_event.insert_path(
            make_event_key_with_prefix(key_path, &self.event_schema.dns_record_schema.name),
            record.name.clone(),
        );
        if let Some(record_type) = record.record_type.clone() {
            self.log_event.insert_path(
                make_event_key_with_prefix(
                    key_path,
                    &self.event_schema.dns_record_schema.record_type,
                ),
                record_type,
            );
        }
        self.log_event.insert_path(
            make_event_key_with_prefix(
                key_path,
                &self.event_schema.dns_record_schema.record_type_id,
            ),
            record.record_type_id as i64,
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(key_path, &self.event_schema.dns_record_schema.ttl),
            record.ttl as i64,
        );
        self.log_event.insert_path(
            make_event_key_with_prefix(key_path, &self.event_schema.dns_record_schema.class),
            record.class.clone(),
        );
        if let Some(rdata) = &record.rdata {
            self.log_event.insert_path(
                make_event_key_with_prefix(key_path, &self.event_schema.dns_record_schema.rdata),
                rdata,
            );
        };
        if let Some(rdata_bytes) = &record.rdata_bytes {
            self.log_event.insert_path(
                make_event_key_with_prefix(key_path, &self.event_schema.dns_record_schema.rdata),
                base64::encode(rdata_bytes),
            );
        };
    }
}

fn make_event_key(name: &str) -> Vec<PathComponent> {
    PathIter::new(name).collect()
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
