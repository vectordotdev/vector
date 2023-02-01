use std::{
    collections::HashSet,
    convert::TryInto,
    fmt::Debug,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use base64::prelude::{Engine as _, BASE64_STANDARD};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use once_cell::sync::Lazy;
use prost::Message;
use snafu::Snafu;
use trust_dns_proto::{
    rr::domain::Name,
    serialize::binary::{BinDecodable, BinDecoder},
};

use crate::{
    event::{LogEvent, Value},
    internal_events::DnstapParseWarning,
    Error, Result,
};

#[allow(warnings, clippy::all, clippy::pedantic, clippy::nursery)]
mod dnstap_proto {
    include!(concat!(env!("OUT_DIR"), "/dnstap.rs"));
}

use dnstap_proto::{
    message::Type as DnstapMessageType, Dnstap, Message as DnstapMessage, SocketFamily,
    SocketProtocol,
};
use lookup::lookup_v2::OwnedValuePath;
use lookup::PathPrefix;

use super::{
    dns_message::{
        DnsRecord, EdnsOptionEntry, OptPseudoSection, QueryHeader, QueryQuestion, UpdateHeader,
        ZoneInfo,
    },
    dns_message_parser::DnsMessageParser,
    schema::DnstapEventSchema,
};

const MAX_DNSTAP_QUERY_MESSAGE_TYPE_ID: i32 = 12;

#[derive(Debug, Snafu)]
enum DnstapParserError {
    #[snafu(display("Unsupported DNSTap message type: {}", "dnstap_message_type_id"))]
    UnsupportedDnstapMessageTypeError { dnstap_message_type_id: i32 },
}

static DNSTAP_MESSAGE_REQUEST_TYPE_IDS: Lazy<HashSet<i32>> = Lazy::new(|| {
    vec![
        DnstapMessageType::AuthQuery as i32,
        DnstapMessageType::ResolverQuery as i32,
        DnstapMessageType::ClientQuery as i32,
        DnstapMessageType::ForwarderQuery as i32,
        DnstapMessageType::StubQuery as i32,
        DnstapMessageType::ToolQuery as i32,
        DnstapMessageType::UpdateQuery as i32,
    ]
    .into_iter()
    .collect()
});
static DNSTAP_MESSAGE_RESPONSE_TYPE_IDS: Lazy<HashSet<i32>> = Lazy::new(|| {
    vec![
        DnstapMessageType::AuthResponse as i32,
        DnstapMessageType::ResolverResponse as i32,
        DnstapMessageType::ClientResponse as i32,
        DnstapMessageType::ForwarderResponse as i32,
        DnstapMessageType::StubResponse as i32,
        DnstapMessageType::ToolResponse as i32,
        DnstapMessageType::UpdateResponse as i32,
    ]
    .into_iter()
    .collect()
});

pub struct DnstapParser<'a> {
    event_schema: &'a DnstapEventSchema,
    parent_key_path: OwnedValuePath,
    log_event: &'a mut LogEvent,
}

pub fn parse_dnstap_data(
    event_schema: &DnstapEventSchema,
    log_event: &mut LogEvent,
    frame: Bytes,
) -> Result<()> {
    DnstapParser::new(event_schema, log_event).parse_dnstap_data(frame)
}

impl<'a> DnstapParser<'a> {
    pub fn new(event_schema: &'a DnstapEventSchema, log_event: &'a mut LogEvent) -> Self {
        Self {
            event_schema,
            parent_key_path: Vec::new().into(),
            log_event,
        }
    }

    fn insert<V>(&mut self, key: &'static str, value: V) -> Option<Value>
    where
        V: Into<Value> + Debug,
    {
        let mut node_path = self.parent_key_path.clone();
        node_path.push_field(key);
        self.log_event
            .insert((PathPrefix::Event, &node_path), value)
    }

    pub fn parse_dnstap_data(&mut self, frame: Bytes) -> Result<()> {
        //parse frame with dnstap protobuf
        let proto_msg = Dnstap::decode(frame.clone())?;

        if let Some(server_id) = proto_msg.identity {
            self.insert(
                self.event_schema
                    .dnstap_root_data_schema()
                    .server_identity(),
                String::from_utf8(server_id).unwrap_or_default(),
            );
        }

        if let Some(version) = proto_msg.version {
            self.insert(
                self.event_schema.dnstap_root_data_schema().server_version(),
                String::from_utf8(version).unwrap_or_default(),
            );
        }

        if let Some(extra) = proto_msg.extra {
            self.insert(
                self.event_schema.dnstap_root_data_schema().extra(),
                String::from_utf8(extra).unwrap_or_default(),
            );
        }

        let dnstap_data_type_id: i32 = proto_msg.r#type;
        let mut need_raw_data = false;
        self.insert(
            self.event_schema.dnstap_root_data_schema().data_type_id(),
            dnstap_data_type_id,
        );

        if let Some(dnstap_data_type) = to_dnstap_data_type(dnstap_data_type_id) {
            self.insert(
                self.event_schema.dnstap_root_data_schema().data_type(),
                dnstap_data_type.clone(),
            );

            if dnstap_data_type == "Message" {
                if let Some(message) = proto_msg.message {
                    if let Err(err) = self.parse_dnstap_message(message) {
                        emit!(DnstapParseWarning { error: &err });
                        need_raw_data = true;
                        self.insert(
                            self.event_schema.dnstap_root_data_schema().error(),
                            err.to_string(),
                        );
                    }
                }
            }
        } else {
            emit!(DnstapParseWarning {
                error: format!("Unknown dnstap data type: {}", dnstap_data_type_id)
            });
            need_raw_data = true;
        }

        if need_raw_data {
            self.insert(
                self.event_schema.dnstap_root_data_schema().raw_data(),
                BASE64_STANDARD.encode(&frame),
            );
        }

        Ok(())
    }

    fn parse_dnstap_message(&mut self, dnstap_message: DnstapMessage) -> Result<()> {
        if let Some(socket_family) = dnstap_message.socket_family {
            self.parse_dnstap_message_socket_family(socket_family, &dnstap_message)?;
        }

        if let Some(query_zone) = dnstap_message.query_zone.as_ref() {
            let mut decoder: BinDecoder = BinDecoder::new(query_zone);
            match Name::read(&mut decoder) {
                Ok(raw_data) => {
                    self.insert(
                        self.event_schema.dnstap_message_schema().query_zone(),
                        raw_data.to_utf8(),
                    );
                }
                Err(error) => return Err(Error::from(error.to_string())),
            }
        }

        let dnstap_message_type_id = dnstap_message.r#type;
        self.insert(
            self.event_schema
                .dnstap_message_schema()
                .dnstap_message_type_id(),
            dnstap_message_type_id,
        );

        self.insert(
            self.event_schema
                .dnstap_message_schema()
                .dnstap_message_type(),
            to_dnstap_message_type(dnstap_message_type_id),
        );

        let request_message_key = self.event_schema.dnstap_message_schema().request_message();
        let response_message_key = self.event_schema.dnstap_message_schema().response_message();

        if let Some(query_time_sec) = dnstap_message.query_time_sec {
            self.parse_dnstap_message_time(
                query_time_sec,
                dnstap_message.query_time_nsec,
                dnstap_message_type_id,
                request_message_key,
                dnstap_message.query_message.as_ref(),
                &DNSTAP_MESSAGE_REQUEST_TYPE_IDS,
            );
        }

        if let Some(response_time_sec) = dnstap_message.response_time_sec {
            self.parse_dnstap_message_time(
                response_time_sec,
                dnstap_message.response_time_nsec,
                dnstap_message_type_id,
                response_message_key,
                dnstap_message.response_message.as_ref(),
                &DNSTAP_MESSAGE_RESPONSE_TYPE_IDS,
            );
        }

        self.parse_dnstap_message_type(
            dnstap_message_type_id,
            dnstap_message,
            request_message_key,
            response_message_key,
        )?;

        Ok(())
    }

    fn parse_dnstap_message_type(
        &mut self,
        dnstap_message_type_id: i32,
        dnstap_message: DnstapMessage,
        request_message_key: &'static str,
        response_message_key: &'static str,
    ) -> Result<()> {
        match dnstap_message_type_id {
            1..=12 => {
                if let Some(query_message) = dnstap_message.query_message {
                    let mut query_message_parser = DnsMessageParser::new(query_message);
                    if let Err(error) =
                        self.parse_dns_query_message(request_message_key, &mut query_message_parser)
                    {
                        self.log_raw_dns_message(
                            request_message_key,
                            query_message_parser.raw_message(),
                        );

                        return Err(error);
                    };
                }

                if let Some(response_message) = dnstap_message.response_message {
                    let mut response_message_parser = DnsMessageParser::new(response_message);
                    if let Err(error) = self
                        .parse_dns_query_message(response_message_key, &mut response_message_parser)
                    {
                        self.log_raw_dns_message(
                            response_message_key,
                            response_message_parser.raw_message(),
                        );

                        return Err(error);
                    };
                }
            }
            13 | 14 => {
                if let Some(update_request_message) = dnstap_message.query_message {
                    let mut update_request_message_parser =
                        DnsMessageParser::new(update_request_message);
                    if let Err(error) = self.parse_dns_update_message(
                        request_message_key,
                        &mut update_request_message_parser,
                    ) {
                        self.log_raw_dns_message(
                            request_message_key,
                            update_request_message_parser.raw_message(),
                        );

                        return Err(error);
                    };
                }

                if let Some(update_response_message) = dnstap_message.response_message {
                    let mut update_response_message_parser =
                        DnsMessageParser::new(update_response_message);
                    if let Err(error) = self.parse_dns_update_message(
                        response_message_key,
                        &mut update_response_message_parser,
                    ) {
                        self.log_raw_dns_message(
                            response_message_key,
                            update_response_message_parser.raw_message(),
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

    fn parse_dnstap_message_time(
        &mut self,
        time_sec: u64,
        time_nsec: Option<u32>,
        dnstap_message_type_id: i32,
        message_key: &str,
        message: Option<&Vec<u8>>,
        type_ids: &HashSet<i32>,
    ) {
        let (time_in_nanosec, query_time_nsec) = match time_nsec {
            Some(nsec) => (time_sec as i64 * 1_000_000_000_i64 + nsec as i64, nsec),
            None => (time_sec as i64 * 1_000_000_000_i64, 0),
        };

        if type_ids.contains(&dnstap_message_type_id) {
            self.log_time(
                self.event_schema.dnstap_root_data_schema().time(),
                time_in_nanosec,
                self.event_schema.dnstap_root_data_schema().time_precision(),
                "ns",
            );

            let timestamp = Utc
                .timestamp_opt(time_sec.try_into().unwrap(), query_time_nsec)
                .single()
                .expect("invalid timestamp");
            self.insert(
                self.event_schema.dnstap_root_data_schema().timestamp(),
                timestamp,
            );
        }

        if message.is_none() {
            self.parent_key_path.push_field(message_key);

            let time_key_name = if dnstap_message_type_id <= MAX_DNSTAP_QUERY_MESSAGE_TYPE_ID {
                self.event_schema.dns_query_message_schema().time()
            } else {
                self.event_schema.dns_update_message_schema().time()
            };

            let time_precision_key_name =
                if dnstap_message_type_id <= MAX_DNSTAP_QUERY_MESSAGE_TYPE_ID {
                    self.event_schema
                        .dns_query_message_schema()
                        .time_precision()
                } else {
                    self.event_schema
                        .dns_update_message_schema()
                        .time_precision()
                };

            self.log_time(
                time_key_name,
                time_in_nanosec,
                time_precision_key_name,
                "ns",
            );

            self.parent_key_path.segments.pop();
        }
    }

    fn parse_dnstap_message_socket_family(
        &mut self,
        socket_family: i32,
        dnstap_message: &DnstapMessage,
    ) -> Result<()> {
        self.insert(
            self.event_schema.dnstap_message_schema().socket_family(),
            to_socket_family_name(socket_family)?.to_string(),
        );

        if let Some(socket_protocol) = dnstap_message.socket_protocol {
            self.insert(
                self.event_schema.dnstap_message_schema().socket_protocol(),
                to_socket_protocol_name(socket_protocol)?.to_string(),
            );
        }

        if let Some(query_address) = dnstap_message.query_address.as_ref() {
            let source_address = if socket_family == 1 {
                let address_buffer: [u8; 4] = query_address[0..4].try_into()?;
                IpAddr::V4(Ipv4Addr::from(address_buffer))
            } else {
                let address_buffer: [u8; 16] = query_address[0..16].try_into()?;
                IpAddr::V6(Ipv6Addr::from(address_buffer))
            };

            self.insert(
                self.event_schema.dnstap_message_schema().query_address(),
                source_address.to_string(),
            );
        }

        if let Some(query_port) = dnstap_message.query_port {
            self.insert(
                self.event_schema.dnstap_message_schema().query_port(),
                query_port,
            );
        }

        if let Some(response_address) = dnstap_message.response_address.as_ref() {
            let response_addr = if socket_family == 1 {
                let address_buffer: [u8; 4] = response_address[0..4].try_into()?;
                IpAddr::V4(Ipv4Addr::from(address_buffer))
            } else {
                let address_buffer: [u8; 16] = response_address[0..16].try_into()?;
                IpAddr::V6(Ipv6Addr::from(address_buffer))
            };

            self.insert(
                self.event_schema.dnstap_message_schema().response_address(),
                response_addr.to_string(),
            );
        }

        Ok(if let Some(response_port) = dnstap_message.response_port {
            self.insert(
                self.event_schema.dnstap_message_schema().response_port(),
                response_port,
            );
        })
    }

    fn log_time(
        &mut self,
        time_key: &'static str,
        time: i64,
        time_precision_key: &'static str,
        time_precision: &str,
    ) {
        self.insert(time_key, time);

        self.insert(time_precision_key, time_precision.to_string());
    }

    fn log_raw_dns_message(&mut self, key_prefix: &'static str, raw_dns_message: &[u8]) {
        self.parent_key_path.push_field(key_prefix);

        self.insert(
            self.event_schema.dns_query_message_schema().raw_data(),
            BASE64_STANDARD.encode(raw_dns_message),
        );

        self.parent_key_path.segments.pop();
    }

    fn parse_dns_query_message(
        &mut self,
        key_prefix: &'static str,
        dns_message_parser: &mut DnsMessageParser,
    ) -> Result<()> {
        let msg = dns_message_parser.parse_as_query_message()?;

        self.parent_key_path.push_field(key_prefix);

        self.insert(
            self.event_schema.dns_query_message_schema().response_code(),
            msg.response_code,
        );

        if let Some(response) = msg.response {
            self.insert(
                self.event_schema.dns_query_message_schema().response(),
                response.to_string(),
            );
        }

        self.log_dns_query_message_header(
            self.event_schema.dns_query_message_schema().header(),
            &msg.header,
        );

        self.log_dns_query_message_query_section(
            self.event_schema
                .dns_query_message_schema()
                .question_section(),
            &msg.question_section,
        );

        self.log_dns_message_record_section(
            self.event_schema
                .dns_query_message_schema()
                .answer_section(),
            &msg.answer_section,
        );

        self.log_dns_message_record_section(
            self.event_schema
                .dns_query_message_schema()
                .authority_section(),
            &msg.authority_section,
        );

        self.log_dns_message_record_section(
            self.event_schema
                .dns_query_message_schema()
                .additional_section(),
            &msg.additional_section,
        );

        self.log_edns(
            self.event_schema
                .dns_query_message_schema()
                .opt_pseudo_section(),
            &msg.opt_pseudo_section,
        );

        self.parent_key_path.segments.pop();
        Ok(())
    }

    fn log_dns_query_message_header(&mut self, parent_key: &'static str, header: &QueryHeader) {
        self.parent_key_path.push_field(parent_key);

        self.insert(self.event_schema.dns_query_header_schema().id(), header.id);

        self.insert(
            self.event_schema.dns_query_header_schema().opcode(),
            header.opcode,
        );

        self.insert(
            self.event_schema.dns_query_header_schema().rcode(),
            u16::from(header.rcode),
        );

        self.insert(self.event_schema.dns_query_header_schema().qr(), header.qr);

        self.insert(self.event_schema.dns_query_header_schema().aa(), header.aa);

        self.insert(self.event_schema.dns_query_header_schema().tc(), header.tc);

        self.insert(self.event_schema.dns_query_header_schema().rd(), header.rd);

        self.insert(self.event_schema.dns_query_header_schema().ra(), header.ra);

        self.insert(self.event_schema.dns_query_header_schema().ad(), header.ad);

        self.insert(self.event_schema.dns_query_header_schema().cd(), header.cd);

        self.insert(
            self.event_schema.dns_query_header_schema().question_count(),
            header.question_count,
        );

        self.insert(
            self.event_schema.dns_query_header_schema().answer_count(),
            header.answer_count,
        );

        self.insert(
            self.event_schema
                .dns_query_header_schema()
                .authority_count(),
            header.authority_count,
        );

        self.insert(
            self.event_schema
                .dns_query_header_schema()
                .additional_count(),
            header.additional_count,
        );

        self.parent_key_path.segments.pop();
    }

    fn log_dns_query_message_query_section(
        &mut self,
        key_path: &'static str,
        questions: &[QueryQuestion],
    ) {
        self.parent_key_path.push_field(key_path);

        for (i, query) in questions.iter().enumerate() {
            self.parent_key_path.push_index(i as isize);
            self.log_dns_query_question(query);
            self.parent_key_path.segments.pop();
        }

        self.parent_key_path.segments.pop();
    }

    fn log_dns_query_question(&mut self, question: &QueryQuestion) {
        self.insert(
            self.event_schema.dns_query_question_schema().name(),
            question.name.clone(),
        );
        if let Some(record_type) = question.record_type.clone() {
            self.insert(
                self.event_schema
                    .dns_query_question_schema()
                    .question_type(),
                record_type,
            );
        }
        self.insert(
            self.event_schema
                .dns_query_question_schema()
                .question_type_id(),
            question.record_type_id,
        );
        self.insert(
            self.event_schema.dns_query_question_schema().class(),
            question.class.clone(),
        );
    }

    fn parse_dns_update_message(
        &mut self,
        key_prefix: &'static str,
        dns_message_parser: &mut DnsMessageParser,
    ) -> Result<()> {
        let msg = dns_message_parser.parse_as_update_message()?;

        self.parent_key_path.push_field(key_prefix);

        self.insert(
            self.event_schema
                .dns_update_message_schema()
                .response_code(),
            msg.response_code,
        );

        if let Some(response) = msg.response {
            self.insert(
                self.event_schema.dns_update_message_schema().response(),
                response.to_string(),
            );
        }

        self.log_dns_update_message_header(
            self.event_schema.dns_update_message_schema().header(),
            &msg.header,
        );

        self.log_dns_update_message_zone_section(
            self.event_schema.dns_update_message_schema().zone_section(),
            &msg.zone_to_update,
        );

        self.log_dns_message_record_section(
            self.event_schema
                .dns_update_message_schema()
                .prerequisite_section(),
            &msg.prerequisite_section,
        );

        self.log_dns_message_record_section(
            self.event_schema
                .dns_update_message_schema()
                .update_section(),
            &msg.update_section,
        );

        self.log_dns_message_record_section(
            self.event_schema
                .dns_update_message_schema()
                .additional_section(),
            &msg.additional_section,
        );

        self.parent_key_path.segments.pop();
        Ok(())
    }

    fn log_dns_update_message_header(&mut self, key_prefix: &'static str, header: &UpdateHeader) {
        self.parent_key_path.push_field(key_prefix);

        self.insert(self.event_schema.dns_update_header_schema().id(), header.id);

        self.insert(
            self.event_schema.dns_update_header_schema().opcode(),
            header.opcode,
        );

        self.insert(
            self.event_schema.dns_update_header_schema().rcode(),
            u16::from(header.rcode),
        );

        self.insert(self.event_schema.dns_update_header_schema().qr(), header.qr);

        self.insert(
            self.event_schema.dns_update_header_schema().zone_count(),
            header.zone_count,
        );

        self.insert(
            self.event_schema
                .dns_update_header_schema()
                .prerequisite_count(),
            header.prerequisite_count,
        );

        self.insert(
            self.event_schema.dns_update_header_schema().update_count(),
            header.update_count,
        );

        self.insert(
            self.event_schema
                .dns_update_header_schema()
                .additional_count(),
            header.additional_count,
        );

        self.parent_key_path.segments.pop();
    }

    fn log_dns_update_message_zone_section(&mut self, key_path: &'static str, zone: &ZoneInfo) {
        self.parent_key_path.push_field(key_path);

        self.insert(
            self.event_schema.dns_update_zone_info_schema().zone_name(),
            zone.name.clone(),
        );
        if let Some(zone_type) = zone.zone_type.clone() {
            self.insert(
                self.event_schema.dns_update_zone_info_schema().zone_type(),
                zone_type,
            );
        }
        self.insert(
            self.event_schema
                .dns_update_zone_info_schema()
                .zone_type_id(),
            zone.zone_type_id,
        );
        self.insert(
            self.event_schema.dns_update_zone_info_schema().zone_class(),
            zone.class.clone(),
        );

        self.parent_key_path.segments.pop();
    }

    fn log_edns(&mut self, key_prefix: &'static str, opt_section: &Option<OptPseudoSection>) {
        self.parent_key_path.push_field(key_prefix);

        if let Some(edns) = opt_section {
            self.insert(
                self.event_schema
                    .dns_message_opt_pseudo_section_schema()
                    .extended_rcode(),
                edns.extended_rcode,
            );
            self.insert(
                self.event_schema
                    .dns_message_opt_pseudo_section_schema()
                    .version(),
                edns.version,
            );
            self.insert(
                self.event_schema
                    .dns_message_opt_pseudo_section_schema()
                    .do_flag(),
                edns.dnssec_ok,
            );
            self.insert(
                self.event_schema
                    .dns_message_opt_pseudo_section_schema()
                    .udp_max_payload_size(),
                edns.udp_max_payload_size,
            );
            self.log_edns_options(
                self.event_schema
                    .dns_message_opt_pseudo_section_schema()
                    .options(),
                &edns.options,
            );
        }

        self.parent_key_path.segments.pop();
    }

    fn log_edns_options(&mut self, key_path: &'static str, options: &[EdnsOptionEntry]) {
        self.parent_key_path.push_field(key_path);

        options.iter().enumerate().for_each(|(i, opt)| {
            self.parent_key_path.push_index(i as isize);
            self.log_edns_opt(opt);
            self.parent_key_path.segments.pop();
        });

        self.parent_key_path.segments.pop();
    }

    fn log_edns_opt(&mut self, opt: &EdnsOptionEntry) {
        self.insert(
            self.event_schema.dns_message_option_schema().opt_code(),
            opt.opt_code,
        );
        self.insert(
            self.event_schema.dns_message_option_schema().opt_name(),
            opt.opt_name.clone(),
        );
        self.insert(
            self.event_schema.dns_message_option_schema().opt_data(),
            opt.opt_data.clone(),
        );
    }

    fn log_dns_message_record_section(&mut self, key_path: &'static str, records: &[DnsRecord]) {
        self.parent_key_path.push_field(key_path);

        for (i, record) in records.iter().enumerate() {
            self.parent_key_path.push_index(i as isize);
            self.log_dns_record(record);
            self.parent_key_path.segments.pop();
        }

        self.parent_key_path.segments.pop();
    }

    fn log_dns_record(&mut self, record: &DnsRecord) {
        self.insert(
            self.event_schema.dns_record_schema().name(),
            record.name.clone(),
        );
        if let Some(record_type) = record.record_type.clone() {
            self.insert(
                self.event_schema.dns_record_schema().record_type(),
                record_type,
            );
        }
        self.insert(
            self.event_schema.dns_record_schema().record_type_id(),
            record.record_type_id,
        );
        self.insert(self.event_schema.dns_record_schema().ttl(), record.ttl);
        self.insert(
            self.event_schema.dns_record_schema().class(),
            record.class.clone(),
        );
        if let Some(rdata) = &record.rdata {
            self.insert(
                self.event_schema.dns_record_schema().rdata(),
                rdata.to_string(),
            );
        };
        if let Some(rdata_bytes) = &record.rdata_bytes {
            self.insert(
                self.event_schema.dns_record_schema().rdata_bytes(),
                BASE64_STANDARD.encode(rdata_bytes),
            );
        };
    }
}

fn to_socket_family_name(socket_family: i32) -> Result<&'static str> {
    if socket_family == SocketFamily::Inet as i32 {
        Ok("INET")
    } else if socket_family == SocketFamily::Inet6 as i32 {
        Ok("INET6")
    } else {
        Err(Error::from(format!(
            "Unknown socket family: {}",
            socket_family
        )))
    }
}

fn to_socket_protocol_name(socket_protocol: i32) -> Result<&'static str> {
    if socket_protocol == SocketProtocol::Udp as i32 {
        Ok("UDP")
    } else if socket_protocol == SocketProtocol::Tcp as i32 {
        Ok("TCP")
    } else if socket_protocol == SocketProtocol::Dot as i32 {
        Ok("DOT")
    } else if socket_protocol == SocketProtocol::Doh as i32 {
        Ok("DOH")
    } else if socket_protocol == SocketProtocol::DnsCryptUdp as i32 {
        Ok("DNSCryptUDP")
    } else if socket_protocol == SocketProtocol::DnsCryptTcp as i32 {
        Ok("DNSCryptTCP")
    } else {
        Err(Error::from(format!(
            "Unknown socket protocol: {}",
            socket_protocol
        )))
    }
}

fn to_dnstap_data_type(data_type_id: i32) -> Option<String> {
    match data_type_id {
        1 => Some(String::from("Message")),
        _ => None,
    }
}

fn to_dnstap_message_type(type_id: i32) -> String {
    match type_id {
        1 => String::from("AuthQuery"),
        2 => String::from("AuthResponse"),
        3 => String::from("ResolverQuery"),
        4 => String::from("ResolverResponse"),
        5 => String::from("ClientQuery"),
        6 => String::from("ClientResponse"),
        7 => String::from("ForwarderQuery"),
        8 => String::from("ForwarderResponse"),
        9 => String::from("StubQuery"),
        10 => String::from("StubResponse"),
        11 => String::from("ToolQuery"),
        12 => String::from("ToolResponse"),
        13 => String::from("UpdateQuery"),
        14 => String::from("UpdateResponse"),
        _ => format!("Unknown dnstap message type: {}", type_id),
    }
}

#[cfg(test)]
mod tests {
    use super::{super::schema::DnstapEventSchema, *};
    use crate::event::Value;

    #[test]
    fn test_parse_dnstap_data_with_query_message() {
        let mut log_event = LogEvent::default();
        let schema = DnstapEventSchema::new();
        let mut parser = DnstapParser::new(&schema, &mut log_event);
        let raw_dnstap_data = "ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zcnoIAxACGAEiEAAAAAAAAA\
        AAAAAAAAAAAAAqECABBQJwlAAAAAAAAAAAADAw8+0CODVA7+zq9wVNMU3WNlI2kwIAAAABAAAAAAABCWZhY2Vib29rMQNjb\
        20AAAEAAQAAKQIAAACAAAAMAAoACOxjCAG9zVgzWgUDY29tAHgB";
        let dnstap_data = BASE64_STANDARD
            .decode(raw_dnstap_data)
            .expect("Invalid base64 encoded data.");
        let parse_result = parser.parse_dnstap_data(Bytes::from(dnstap_data));
        assert!(parse_result.is_ok());
        assert!(log_event
            .all_fields()
            .unwrap()
            .any(|(key, value)| key == "time"
                && match *value {
                    Value::Integer(time) => time == 1_593_489_007_920_014_129,
                    _ => false,
                }));
        assert!(log_event
            .all_fields()
            .unwrap()
            .any(|(key, value)| key == "timestamp"
                && match *value {
                    Value::Timestamp(timestamp) =>
                        timestamp.timestamp_nanos() == 1_593_489_007_920_014_129,
                    _ => false,
                }));
        assert!(log_event
            .all_fields()
            .unwrap()
            .any(|(key, value)| key == "requestData.header.qr"
                && match *value {
                    Value::Integer(qr) => qr == 0,
                    _ => false,
                }));
        assert!(log_event.all_fields().unwrap().any(|(key, value)| key
            == "requestData.opt.udpPayloadSize"
            && match *value {
                Value::Integer(udp_payload_size) => udp_payload_size == 512,
                _ => false,
            }));
        assert!(log_event.all_fields().unwrap().any(|(key, value)| key
            == "requestData.question[0].domainName"
            && match value {
                Value::Bytes(domain_name) => *domain_name == Bytes::from_static(b"facebook1.com."),
                _ => false,
            }));
    }

    #[test]
    fn test_parse_dnstap_data_with_update_message() {
        let mut log_event = LogEvent::default();
        let schema = DnstapEventSchema::new();
        let mut parser = DnstapParser::new(&schema, &mut log_event);
        let raw_dnstap_data = "ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zcmsIDhABGAEiBH8AAA\
        EqBH8AAAEwrG44AEC+iu73BU14gfofUh1wi6gAAAEAAAAAAAAHZXhhbXBsZQNjb20AAAYAAWC+iu73BW0agDwvch1wi6gAA\
        AEAAAAAAAAHZXhhbXBsZQNjb20AAAYAAXgB";
        let dnstap_data = BASE64_STANDARD
            .decode(raw_dnstap_data)
            .expect("Invalid base64 encoded data.");
        let parse_result = parser.parse_dnstap_data(Bytes::from(dnstap_data));
        assert!(parse_result.is_ok());
        assert!(log_event
            .all_fields()
            .unwrap()
            .any(|(key, value)| key == "time"
                && match *value {
                    Value::Integer(time) => time == 1_593_541_950_792_494_106,
                    _ => false,
                }));
        assert!(log_event
            .all_fields()
            .unwrap()
            .any(|(key, value)| key == "timestamp"
                && match *value {
                    Value::Timestamp(timestamp) =>
                        timestamp.timestamp_nanos() == 1_593_541_950_792_494_106,
                    _ => false,
                }));
        assert!(log_event
            .all_fields()
            .unwrap()
            .any(|(key, value)| key == "requestData.header.qr"
                && match *value {
                    Value::Integer(qr) => qr == 1,
                    _ => false,
                }));
        assert!(log_event
            .all_fields()
            .unwrap()
            .any(|(key, value)| key == "messageType"
                && match value {
                    Value::Bytes(data_type) => *data_type == Bytes::from_static(b"UpdateResponse"),
                    _ => false,
                }));
        assert!(log_event.all_fields().unwrap().any(|(key, value)| key
            == "requestData.zone.zName"
            && match value {
                Value::Bytes(domain_name) => *domain_name == Bytes::from_static(b"example.com."),
                _ => false,
            }));
    }

    #[test]
    fn test_parse_dnstap_data_with_invalid_data() {
        let mut log_event = LogEvent::default();
        let schema = DnstapEventSchema::new();
        let mut parser = DnstapParser::new(&schema, &mut log_event);
        let e = parser
            .parse_dnstap_data(Bytes::from(vec![1, 2, 3]))
            .expect_err("Expected TrustDnsError.");
        assert!(e.to_string().contains("Protobuf message"));
    }

    #[test]
    fn test_get_socket_family_name() {
        assert_eq!("INET", to_socket_family_name(1).unwrap());
        assert_eq!("INET6", to_socket_family_name(2).unwrap());
        assert!(to_socket_family_name(3).is_err());
    }

    #[test]
    fn test_get_socket_protocol_name() {
        assert_eq!("UDP", to_socket_protocol_name(1).unwrap());
        assert_eq!("TCP", to_socket_protocol_name(2).unwrap());
        assert_eq!("DOT", to_socket_protocol_name(3).unwrap());
        assert_eq!("DOH", to_socket_protocol_name(4).unwrap());
        assert_eq!("DNSCryptUDP", to_socket_protocol_name(5).unwrap());
        assert_eq!("DNSCryptTCP", to_socket_protocol_name(6).unwrap());
        assert!(to_socket_protocol_name(7).is_err());
    }
}
