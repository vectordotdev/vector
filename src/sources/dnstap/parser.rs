use crate::{
    event::{LogEvent, PathComponent, Value},
    Error, Result,
};
use bytes::Bytes;
use prost::Message;
use snafu::Snafu;
use std::convert::TryInto;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use trust_dns_proto::{
    rr::domain::Name,
    serialize::binary::{BinDecodable, BinDecoder},
};
mod dnstap_proto {
    include!(concat!(env!("OUT_DIR"), "/dnstap.rs"));
}

use dnstap_proto::{
    dnstap::Type as DnstapDataType, message::Type as DnstapMessageType, Dnstap,
    Message as DnstapMessage,
};

use super::dns_message::{
    DnsRecord, EdnsOptionEntry, OptPseudoSection, QueryHeader, QueryQuestion, UpdateHeader,
    ZoneInfo,
};
use super::dns_message_parser::{parse_dns_query_message, parse_dns_update_message};
use super::schema::DnstapEventSchema;

#[derive(Debug, Snafu)]
enum DnstapParserError {
    #[snafu(display("Unsupported DNSTap message type: {}", "dnstap_message_type_id"))]
    UnsupportedDnstapMessageTypeError { dnstap_message_type_id: i32 },
}

pub struct DnstapParser<'a> {
    event_schema: &'a DnstapEventSchema,
    parent_key_path: Vec<PathComponent>,
    log_event: &'a mut LogEvent,
}

impl<'a> DnstapParser<'a> {
    pub fn new(event_schema: &'a DnstapEventSchema, log_event: &'a mut LogEvent) -> Self {
        Self {
            event_schema,
            parent_key_path: Vec::new(),
            log_event,
        }
    }

    fn insert<V>(&mut self, key: &String, value: V) -> Option<Value>
    where
        V: Into<Value>,
    {
        let mut node_path = self.parent_key_path.clone();
        node_path.push(PathComponent::Key(key.clone()));
        self.log_event.insert_path(node_path, value)
    }

    pub fn parse_dnstap_data(&mut self, frame: Bytes) -> Result<()> {
        //parse frame with dnstap protobuf
        let proto_msg = Dnstap::decode(frame.clone())?;

        if let Some(server_id) = proto_msg.identity {
            self.insert(
                &self.event_schema.dnstap_root_data_schema.server_identity,
                String::from_utf8(server_id).unwrap_or_default(),
            );
        }

        if let Some(version) = proto_msg.version {
            self.insert(
                &self.event_schema.dnstap_root_data_schema.server_version,
                String::from_utf8(version).unwrap_or_default(),
            );
        }

        if let Some(extra) = proto_msg.extra {
            self.insert(
                &self.event_schema.dnstap_root_data_schema.extra,
                String::from_utf8(extra).unwrap_or_default(),
            );
        }

        let dnstap_data_type: i32 = proto_msg.r#type;
        // the raw value is reserved intentionally to ensure forward-compatibility
        let mut need_raw_data = false;
        self.insert(
            &self.event_schema.dnstap_root_data_schema.data_type,
            dnstap_data_type,
        );
        if dnstap_data_type == DnstapDataType::Message as i32 {
            if let Some(message) = proto_msg.message {
                if let Err(err) = self.parse_dnstap_message(message) {
                    error!(target: "dnstap event", "failed to parse dnstap message: {}", err.to_string());
                    need_raw_data = true;
                    self.insert(
                        &self.event_schema.dnstap_root_data_schema.error,
                        err.to_string(),
                    );
                }
            }
        } else {
            need_raw_data = true;
        }

        if need_raw_data {
            self.insert(
                &self.event_schema.dnstap_root_data_schema.raw_data,
                base64::encode(&frame.to_vec()),
            );
        }

        Ok(())
    }

    fn parse_dnstap_message(&mut self, dnstap_message: DnstapMessage) -> Result<()> {
        if let Some(socket_family) = dnstap_message.socket_family {
            // the raw value is reserved intentionally to ensure forward-compatibility
            self.insert(
                &self.event_schema.dnstap_message_schema.socket_family,
                socket_family,
            );

            if let Some(socket_protocol) = dnstap_message.socket_protocol {
                // the raw value is reserved intentionally to ensure forward-compatibility
                self.insert(
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

                self.insert(
                    &self.event_schema.dnstap_message_schema.query_address,
                    source_address.to_string(),
                );
            }

            if let Some(query_port) = dnstap_message.query_port {
                self.insert(
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

                self.insert(
                    &self.event_schema.dnstap_message_schema.response_address,
                    response_addr.to_string(),
                );
            }

            if let Some(response_port) = dnstap_message.response_port {
                self.insert(
                    &self.event_schema.dnstap_message_schema.response_port,
                    response_port as i64,
                );
            }
        }

        if let Some(query_zone) = dnstap_message.query_zone {
            let mut decoder: BinDecoder = BinDecoder::new(&query_zone);
            match Name::read(&mut decoder) {
                Ok(raw_data) => {
                    self.insert(
                        &self.event_schema.dnstap_message_schema.query_zone,
                        raw_data.to_utf8(),
                    );
                }
                Err(error) => return Err(Error::from(error.to_string())),
            }
        }

        // the raw value is reserved intentionally to ensure forward-compatibility
        let dnstap_message_type_id = dnstap_message.r#type;
        self.insert(
            &self
                .event_schema
                .dnstap_message_schema
                .dnstap_message_type_id,
            dnstap_message_type_id as i64,
        );

        if let Some(dnstap_message_type) = to_dnstap_message_type(dnstap_message_type_id) {
            self.insert(
                &self.event_schema.dnstap_message_schema.dnstap_message_type,
                format!("{:?}", dnstap_message_type),
            );
        }

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

        let update_request_message_key = self
            .event_schema
            .dnstap_message_schema
            .update_request_message
            .to_string();
        let update_response_message_key = self
            .event_schema
            .dnstap_message_schema
            .update_response_message
            .to_string();

        if let Some(query_time_sec) = dnstap_message.query_time_sec {
            let mut timestamp_nanosec: i64 = query_time_sec as i64 * 1_000_000_000 as i64;

            if let Some(query_time_nsec) = dnstap_message.query_time_nsec {
                timestamp_nanosec += query_time_nsec as i64;
            }

            if [1, 3, 5, 7, 9, 11, 13].contains(&dnstap_message_type_id) {
                self.log_timestamp(
                    &self.event_schema.dnstap_root_data_schema.timestamp,
                    timestamp_nanosec,
                    &self.event_schema.dnstap_root_data_schema.time_precision,
                    "ns",
                );
            }

            if dnstap_message.query_message != None {
                if dnstap_message_type_id <= 12 {
                    self.parent_key_path
                        .push(PathComponent::Key(query_message_key.clone()));
                } else {
                    self.parent_key_path
                        .push(PathComponent::Key(update_request_message_key.clone()));
                };

                let timestamp_key_name = if dnstap_message_type_id <= 12 {
                    &self.event_schema.dns_query_message_schema.timestamp
                } else {
                    &self.event_schema.dns_update_message_schema.timestamp
                };

                let time_precision_key_name = if dnstap_message_type_id <= 12 {
                    &self.event_schema.dns_query_message_schema.time_precision
                } else {
                    &self.event_schema.dns_update_message_schema.time_precision
                };

                self.log_timestamp(
                    timestamp_key_name,
                    timestamp_nanosec,
                    time_precision_key_name,
                    "ns",
                );

                self.parent_key_path.pop();
            }
        }

        if let Some(response_time_sec) = dnstap_message.response_time_sec {
            let mut timestamp_nanosec: i64 = response_time_sec as i64 * 1_000_000_000 as i64;

            if let Some(response_time_nsec) = dnstap_message.response_time_nsec {
                timestamp_nanosec += response_time_nsec as i64;
            }

            if [2, 4, 6, 8, 10, 12, 14].contains(&dnstap_message_type_id) {
                self.log_timestamp(
                    &self.event_schema.dnstap_root_data_schema.timestamp,
                    timestamp_nanosec,
                    &self.event_schema.dnstap_root_data_schema.time_precision,
                    "ns",
                );
            }

            if dnstap_message.response_message != None {
                if dnstap_message_type_id <= 12 {
                    self.parent_key_path
                        .push(PathComponent::Key(response_message_key.clone()));
                } else {
                    self.parent_key_path
                        .push(PathComponent::Key(update_response_message_key.clone()));
                };

                let timestamp_key_name = if dnstap_message_type_id <= 12 {
                    &self.event_schema.dns_query_message_schema.timestamp
                } else {
                    &self.event_schema.dns_update_message_schema.timestamp
                };

                let time_precision_key_name = if dnstap_message_type_id <= 12 {
                    &self.event_schema.dns_query_message_schema.time_precision
                } else {
                    &self.event_schema.dns_update_message_schema.time_precision
                };

                self.log_timestamp(
                    timestamp_key_name,
                    timestamp_nanosec,
                    time_precision_key_name,
                    "ns",
                );

                self.parent_key_path.pop();
            }
        }

        match dnstap_message_type_id {
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
            13 | 14 => {
                if let Some(update_request_message) = dnstap_message.query_message {
                    if let Err(error) = self.parse_dns_update_message(
                        &update_request_message_key,
                        &update_request_message,
                    ) {
                        self.log_raw_dns_message(
                            &update_request_message_key,
                            &update_request_message,
                        );

                        return Err(error);
                    };
                }

                if let Some(udpate_response_message) = dnstap_message.response_message {
                    if let Err(error) = self.parse_dns_update_message(
                        &update_response_message_key,
                        &udpate_response_message,
                    ) {
                        self.log_raw_dns_message(
                            &update_response_message_key,
                            &udpate_response_message,
                        );

                        return Err(error);
                    };
                }
            }
            _ => {
                return Err(Box::new(
                    DnstapParserError::UnsupportedDnstapMessageTypeError {
                        dnstap_message_type_id,
                    },
                ));
            }
        }

        Ok(())
    }

    fn log_timestamp(
        &mut self,
        timestamp_key: &str,
        timestamp: i64,
        time_precision_key: &str,
        time_precision: &str,
    ) {
        self.insert(&String::from(timestamp_key), timestamp);

        self.insert(&String::from(time_precision_key), time_precision);
    }

    fn log_raw_dns_message(&mut self, key_prefix: &str, raw_dns_message: &Vec<u8>) {
        self.parent_key_path
            .push(PathComponent::Key(String::from(key_prefix)));

        self.insert(
            &self.event_schema.dns_query_message_schema.raw_data,
            base64::encode(raw_dns_message),
        );

        self.parent_key_path.pop();
    }

    fn parse_dns_query_message(
        &mut self,
        key_prefix: &str,
        raw_dns_message: &Vec<u8>,
    ) -> Result<()> {
        let msg = parse_dns_query_message(raw_dns_message)?;

        self.parent_key_path
            .push(PathComponent::Key(String::from(key_prefix)));

        self.insert(
            &self.event_schema.dns_query_message_schema.response_code,
            msg.response_code as i64,
        );

        if let Some(response) = msg.response {
            self.insert(
                &self.event_schema.dns_query_message_schema.response,
                response,
            );
        }

        self.log_dns_query_message_header(
            &self.event_schema.dns_query_message_schema.header,
            &msg.header,
        );

        self.log_dns_query_message_query_section(
            &self.event_schema.dns_query_message_schema.question_section,
            &msg.question_section,
        );

        self.log_dns_message_record_section(
            &self.event_schema.dns_query_message_schema.answer_section,
            &msg.answer_section,
        );

        self.log_dns_message_record_section(
            &self.event_schema.dns_query_message_schema.authority_section,
            &msg.authority_section,
        );

        self.log_dns_message_record_section(
            &self
                .event_schema
                .dns_query_message_schema
                .additional_section,
            &msg.additional_section,
        );

        self.log_edns(
            &self
                .event_schema
                .dns_query_message_schema
                .opt_pseudo_section,
            &msg.opt_pserdo_section,
        );

        self.parent_key_path.pop();
        Ok(())
    }

    fn log_dns_query_message_header(&mut self, parent_key: &str, header: &QueryHeader) {
        self.parent_key_path
            .push(PathComponent::Key(String::from(parent_key)));

        self.insert(
            &self.event_schema.dns_query_header_schema.id,
            header.id as i64,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.opcode,
            header.opcode as i64,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.rcode,
            header.rcode as i64,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.qr,
            header.qr as i64,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.aa,
            header.aa as bool,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.tc,
            header.tc as bool,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.rd,
            header.rd as bool,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.ra,
            header.ra as bool,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.ad,
            header.ad as bool,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.cd,
            header.cd as bool,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.question_count,
            header.question_count as i64,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.answer_count,
            header.answer_count as i64,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.authority_count,
            header.authority_count as i64,
        );

        self.insert(
            &self.event_schema.dns_query_header_schema.additional_count,
            header.additional_count as i64,
        );

        self.parent_key_path.pop();
    }

    fn log_dns_query_message_query_section(
        &mut self,
        key_path: &str,
        questions: &Vec<QueryQuestion>,
    ) {
        self.parent_key_path
            .push(PathComponent::Key(String::from(key_path)));

        for (i, query) in questions.iter().enumerate() {
            self.parent_key_path.push(PathComponent::Index(i));
            self.log_dns_query_question(query);
            self.parent_key_path.pop();
        }

        self.parent_key_path.pop();
    }

    fn log_dns_query_question(&mut self, question: &QueryQuestion) {
        self.insert(
            &self.event_schema.dns_record_schema.name,
            question.name.clone(),
        );
        if let Some(record_type) = question.record_type.clone() {
            self.insert(
                &self.event_schema.dns_record_schema.record_type,
                record_type,
            );
        }
        self.insert(
            &self.event_schema.dns_record_schema.record_type_id,
            question.record_type_id as i64,
        );
        self.insert(
            &self.event_schema.dns_record_schema.class,
            question.class.clone(),
        );
    }

    fn parse_dns_update_message(
        &mut self,
        key_prefix: &str,
        raw_dns_message: &Vec<u8>,
    ) -> Result<()> {
        let msg = parse_dns_update_message(raw_dns_message)?;

        self.parent_key_path
            .push(PathComponent::Key(String::from(key_prefix)));

        self.insert(
            &self.event_schema.dns_update_message_schema.response_code,
            msg.response_code as i64,
        );

        if let Some(response) = msg.response {
            self.insert(
                &self.event_schema.dns_update_message_schema.response,
                response,
            );
        }

        self.log_dns_update_message_header(
            &self.event_schema.dns_update_message_schema.header,
            &msg.header,
        );

        self.log_dns_update_message_zone_section(
            &self.event_schema.dns_update_message_schema.zone_section,
            &msg.zone_to_update,
        );

        self.log_dns_message_record_section(
            &self
                .event_schema
                .dns_update_message_schema
                .prerequisite_section,
            &msg.prerequisite_section,
        );

        self.log_dns_message_record_section(
            &self.event_schema.dns_update_message_schema.update_section,
            &msg.update_section,
        );

        self.log_dns_message_record_section(
            &self
                .event_schema
                .dns_update_message_schema
                .additional_section,
            &msg.additional_section,
        );

        self.log_edns(
            &self
                .event_schema
                .dns_update_message_schema
                .opt_pseudo_section,
            &msg.opt_pserdo_section,
        );

        self.parent_key_path.pop();
        Ok(())
    }

    fn log_dns_update_message_header(&mut self, key_prefix: &str, header: &UpdateHeader) {
        self.parent_key_path
            .push(PathComponent::Key(String::from(key_prefix)));

        self.insert(
            &self.event_schema.dns_update_header_schema.id,
            header.id as i64,
        );

        self.insert(
            &self.event_schema.dns_update_header_schema.opcode,
            header.opcode as i64,
        );

        self.insert(
            &self.event_schema.dns_update_header_schema.rcode,
            header.rcode as i64,
        );

        self.insert(
            &self.event_schema.dns_update_header_schema.qr,
            header.qr as i64,
        );

        self.insert(
            &self.event_schema.dns_update_header_schema.zone_count,
            header.zone_count as i64,
        );

        self.insert(
            &self
                .event_schema
                .dns_update_header_schema
                .prerequisite_count,
            header.prerequisite_count as i64,
        );

        self.insert(
            &self.event_schema.dns_update_header_schema.udpate_count,
            header.update_count as i64,
        );

        self.insert(
            &self.event_schema.dns_update_header_schema.additional_count,
            header.additional_count as i64,
        );

        self.parent_key_path.pop();
    }

    fn log_dns_update_message_zone_section(&mut self, key_path: &str, zone: &ZoneInfo) {
        self.parent_key_path
            .push(PathComponent::Key(String::from(key_path)));

        self.insert(&self.event_schema.dns_record_schema.name, zone.name.clone());
        if let Some(zone_type) = zone.zone_type.clone() {
            self.insert(&self.event_schema.dns_record_schema.record_type, zone_type);
        }
        self.insert(
            &self.event_schema.dns_record_schema.record_type_id,
            zone.zone_type_id as i64,
        );
        self.insert(
            &self.event_schema.dns_record_schema.class,
            zone.class.clone(),
        );

        self.parent_key_path.pop();
    }

    fn log_edns(&mut self, key_prefix: &str, opt_section: &Option<OptPseudoSection>) {
        self.parent_key_path
            .push(PathComponent::Key(String::from(key_prefix)));

        if let Some(edns) = opt_section {
            self.insert(
                &self
                    .event_schema
                    .dns_message_opt_pseudo_section_schema
                    .extended_rcode,
                edns.extended_rcode as i64,
            );
            self.insert(
                &self
                    .event_schema
                    .dns_message_opt_pseudo_section_schema
                    .version,
                edns.version as i64,
            );
            self.insert(
                &self
                    .event_schema
                    .dns_message_opt_pseudo_section_schema
                    .do_flag,
                edns.dnssec_ok as bool,
            );
            self.insert(
                &self
                    .event_schema
                    .dns_message_opt_pseudo_section_schema
                    .udp_max_payload_size,
                edns.udp_max_payload_size as i64,
            );
            self.log_edns_options(
                &self
                    .event_schema
                    .dns_message_opt_pseudo_section_schema
                    .options,
                &edns.options,
            );
        }

        self.parent_key_path.pop();
    }

    fn log_edns_options(&mut self, key_path: &str, options: &Vec<EdnsOptionEntry>) {
        self.parent_key_path
            .push(PathComponent::Key(String::from(key_path)));

        options.iter().enumerate().for_each(|(i, opt)| {
            self.parent_key_path.push(PathComponent::Index(i));
            self.log_edns_opt(opt);
            self.parent_key_path.pop();
        });

        self.parent_key_path.pop();
    }

    fn log_edns_opt(&mut self, opt: &EdnsOptionEntry) {
        self.insert(
            &self.event_schema.dns_message_option_schema.opt_code,
            opt.opt_code as i64,
        );
        self.insert(
            &self.event_schema.dns_message_option_schema.opt_name,
            opt.opt_name.clone(),
        );
        self.insert(
            &self.event_schema.dns_message_option_schema.opt_data,
            opt.opt_data.clone(),
        );
    }

    fn log_dns_message_record_section(&mut self, key_path: &str, records: &Vec<DnsRecord>) {
        self.parent_key_path
            .push(PathComponent::Key(String::from(key_path)));

        for (i, record) in records.iter().enumerate() {
            self.parent_key_path.push(PathComponent::Index(i));
            self.log_dns_record(record);
            self.parent_key_path.pop();
        }

        self.parent_key_path.pop();
    }

    fn log_dns_record(&mut self, record: &DnsRecord) {
        self.insert(
            &self.event_schema.dns_record_schema.name,
            record.name.clone(),
        );
        if let Some(record_type) = record.record_type.clone() {
            self.insert(
                &self.event_schema.dns_record_schema.record_type,
                record_type,
            );
        }
        self.insert(
            &self.event_schema.dns_record_schema.record_type_id,
            record.record_type_id as i64,
        );
        self.insert(&self.event_schema.dns_record_schema.ttl, record.ttl as i64);
        self.insert(
            &self.event_schema.dns_record_schema.class,
            record.class.clone(),
        );
        if let Some(rdata) = &record.rdata {
            self.insert(&self.event_schema.dns_record_schema.rdata, rdata);
        };
        if let Some(rdata_bytes) = &record.rdata_bytes {
            self.insert(
                &self.event_schema.dns_record_schema.rdata,
                base64::encode(rdata_bytes),
            );
        };
    }
}

fn to_dnstap_message_type(type_id: i32) -> Option<DnstapMessageType> {
    match type_id {
        1 => Some(DnstapMessageType::AuthQuery),
        2 => Some(DnstapMessageType::AuthResponse),
        3 => Some(DnstapMessageType::ResolverQuery),
        4 => Some(DnstapMessageType::ResolverResponse),
        5 => Some(DnstapMessageType::ClientQuery),
        6 => Some(DnstapMessageType::ClientResponse),
        7 => Some(DnstapMessageType::ForwarderQuery),
        8 => Some(DnstapMessageType::ForwarderResponse),
        9 => Some(DnstapMessageType::StubQuery),
        10 => Some(DnstapMessageType::StubResponse),
        11 => Some(DnstapMessageType::ToolQuery),
        12 => Some(DnstapMessageType::ToolResponse),
        13 => Some(DnstapMessageType::UpdateQuery),
        14 => Some(DnstapMessageType::UpdateResponse),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{super::schema::DnstapEventSchema, *};
    use crate::event::Event;
    use crate::event::Value;
    use std::time::Instant;

    #[test]
    fn test_parse_dnstap_data_with_query_message() {
        let mut event = Event::new_empty_log();
        let log_event = event.as_mut_log();
        let schema = DnstapEventSchema::new();
        let mut parser = DnstapParser::new(&schema, log_event);
        let raw_dnstap_data = "ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zcnoIAxACGAEiEAAAAAAAAA\
        AAAAAAAAAAAAAqECABBQJwlAAAAAAAAAAAADAw8+0CODVA7+zq9wVNMU3WNlI2kwIAAAABAAAAAAABCWZhY2Vib29rMQNjb\
        20AAAEAAQAAKQIAAACAAAAMAAoACOxjCAG9zVgzWgUDY29tAHgB";
        if let Ok(dnstap_data) = base64::decode(raw_dnstap_data) {
            let parse_result = parser.parse_dnstap_data(Bytes::from(dnstap_data));
            assert!(parse_result.is_ok());
            assert!(log_event
                .all_fields()
                .find(|(key, value)| key == "time"
                    && match *value {
                        Value::Integer(time) => *time == 1_593_489_007_920_014_129,
                        _ => false,
                    })
                .is_some());
            assert!(log_event
                .all_fields()
                .find(|(key, value)| key == "requestData.header.qr"
                    && match *value {
                        Value::Integer(qr) => *qr == 0,
                        _ => false,
                    })
                .is_some());
            assert!(log_event
                .all_fields()
                .find(|(key, value)| key == "requestData.opt.udpPayloadSize"
                    && match *value {
                        Value::Integer(udp_payload_size) => *udp_payload_size == 512,
                        _ => false,
                    })
                .is_some());
            assert!(log_event
                .all_fields()
                .find(|(key, value)| key == "requestData.question[0].domainName"
                    && match *value {
                        Value::Bytes(domain_name) =>
                            *domain_name == Bytes::from_static(b"facebook1.com."),
                        _ => false,
                    })
                .is_some());
        } else {
            error!("Invalid base64 encoded data");
        }
    }

    #[test]
    fn test_parse_dnstap_data_with_update_message() {
        let mut event = Event::new_empty_log();
        let log_event = event.as_mut_log();
        let schema = DnstapEventSchema::new();
        let mut parser = DnstapParser::new(&schema, log_event);
        let raw_dnstap_data = "ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zcmsIDhABGAEiBH8AAA\
        EqBH8AAAEwrG44AEC+iu73BU14gfofUh1wi6gAAAEAAAAAAAAHZXhhbXBsZQNjb20AAAYAAWC+iu73BW0agDwvch1wi6gAA\
        AEAAAAAAAAHZXhhbXBsZQNjb20AAAYAAXgB";
        if let Ok(dnstap_data) = base64::decode(raw_dnstap_data) {
            let parse_result = parser.parse_dnstap_data(Bytes::from(dnstap_data));
            assert!(parse_result.is_ok());
            assert!(log_event
                .all_fields()
                .find(|(key, value)| key == "time"
                    && match *value {
                        Value::Integer(time) => *time == 1_593_541_950_792_494_106,
                        _ => false,
                    })
                .is_some());
            assert!(log_event
                .all_fields()
                .find(|(key, value)| key == "updateRequestData.header.qr"
                    && match *value {
                        Value::Integer(qr) => *qr == 1,
                        _ => false,
                    })
                .is_some());
            assert!(log_event
                .all_fields()
                .find(|(key, value)| key == "messageType"
                    && match *value {
                        Value::Bytes(data_type) =>
                            *data_type == Bytes::from_static(b"UpdateResponse"),
                        _ => false,
                    })
                .is_some());
            assert!(log_event
                .all_fields()
                .find(|(key, value)| key == "updateRequestData.zone.domainName"
                    && match *value {
                        Value::Bytes(domain_name) =>
                            *domain_name == Bytes::from_static(b"example.com."),
                        _ => false,
                    })
                .is_some());
        } else {
            error!("Invalid base64 encoded data");
        }
    }

    #[test]
    fn test_parse_dnstap_data_with_invalid_data() {
        let mut event = Event::new_empty_log();
        let log_event = event.as_mut_log();
        let schema = DnstapEventSchema::new();
        let mut parser = DnstapParser::new(&schema, log_event);
        if let Err(e) = parser.parse_dnstap_data(Bytes::from(vec![1, 2, 3])) {
            assert!(e.to_string().contains("Protobuf message"));
        } else {
            error!("Expected TrustDnsError");
        }
    }

    #[test]
    #[ignore]
    fn benchmark_dnstap_parser_with_queries() {
        let mut event = Event::new_empty_log();
        let log_event = event.as_mut_log();
        let schema = DnstapEventSchema::new();
        let mut parser = DnstapParser::new(&schema, log_event);
        let raw_dnstap_data = "ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zcnoIAxACGAEiEAAAAAAAAA\
        AAAAAAAAAAAAAqECABBQJwlAAAAAAAAAAAADAw8+0CODVA7+zq9wVNMU3WNlI2kwIAAAABAAAAAAABCWZhY2Vib29rMQNjb\
        20AAAEAAQAAKQIAAACAAAAMAAoACOxjCAG9zVgzWgUDY29tAHgB";
        if let Ok(dnstap_data) = base64::decode(raw_dnstap_data) {
            let start = Instant::now();
            let num = 10_000;
            for _ in 0..num {
                let parse_result = parser.parse_dnstap_data(Bytes::from(dnstap_data.clone()));
                assert!(parse_result.is_ok());
            }
            let time_taken = Instant::now().duration_since(start);
            println!(
                "Time taken to parse {} dnstap events carrying DNS query messages: {:#?}",
                num, time_taken
            );
        } else {
            error!("Invalid base64 encoded data");
        }
    }

    #[test]
    #[ignore]
    fn benchmark_dnstap_parser_with_udpates() {
        let mut event = Event::new_empty_log();
        let log_event = event.as_mut_log();
        let schema = DnstapEventSchema::new();
        let mut parser = DnstapParser::new(&schema, log_event);
        let raw_dnstap_data = "ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zcmsIDhABGAEiBH8AAA\
        EqBH8AAAEwrG44AEC+iu73BU14gfofUh1wi6gAAAEAAAAAAAAHZXhhbXBsZQNjb20AAAYAAWC+iu73BW0agDwvch1wi6gAA\
        AEAAAAAAAAHZXhhbXBsZQNjb20AAAYAAXgB";
        if let Ok(dnstap_data) = base64::decode(raw_dnstap_data) {
            let start = Instant::now();
            let num = 10_000;
            for _ in 0..num {
                let parse_result = parser.parse_dnstap_data(Bytes::from(dnstap_data.clone()));
                assert!(parse_result.is_ok());
            }
            let time_taken = Instant::now().duration_since(start);
            println!(
                "Time taken to parse {} dnstap events carrying DNS update messages: {:#?}",
                num, time_taken
            );
        } else {
            error!("Invalid base64 encoded data");
        }
    }
}
