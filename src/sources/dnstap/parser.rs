use std::{
    collections::HashSet,
    convert::TryInto,
    fmt::Debug,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use base64::prelude::{Engine as _, BASE64_STANDARD};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use dnsmsg_parser::{dns_message_parser::DnsParserOptions, ede::EDE};
use hickory_proto::{
    rr::domain::Name,
    serialize::binary::{BinDecodable, BinDecoder},
};
use once_cell::sync::Lazy;
use prost::Message;
use snafu::Snafu;
use vrl::{owned_value_path, path};

use crate::{
    event::{LogEvent, Value},
    internal_events::DnstapParseWarning,
    Error, Result,
};

#[allow(warnings, clippy::all, clippy::pedantic, clippy::nursery)]
mod dnstap_proto {
    include!(concat!(env!("OUT_DIR"), "/dnstap.rs"));
}

use crate::sources::dnstap::schema::DNSTAP_VALUE_PATHS;
use dnstap_proto::{
    message::Type as DnstapMessageType, Dnstap, Message as DnstapMessage, SocketFamily,
    SocketProtocol,
};
use vector_lib::config::log_schema;
use vector_lib::lookup::lookup_v2::ValuePath;
use vector_lib::lookup::PathPrefix;

use super::{
    dns_message::{
        DnsRecord, EdnsOptionEntry, OptPseudoSection, QueryHeader, QueryQuestion, UpdateHeader,
        ZoneInfo,
    },
    dns_message_parser::DnsMessageParser,
};

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

#[derive(Default)]
pub struct DnstapParser;

impl DnstapParser {
    fn insert<'a, V>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        path: impl ValuePath<'a>,
        value: V,
    ) -> Option<Value>
    where
        V: Into<Value> + Debug,
    {
        event.insert((PathPrefix::Event, prefix.concat(path)), value)
    }

    pub fn parse(
        event: &mut LogEvent,
        frame: Bytes,
        parsing_options: DnsParserOptions,
    ) -> Result<()> {
        //parse frame with dnstap protobuf
        let proto_msg = Dnstap::decode(frame.clone())?;
        let root = owned_value_path!();
        if let Some(server_id) = proto_msg.identity {
            DnstapParser::insert(
                event,
                &root,
                &DNSTAP_VALUE_PATHS.server_identity,
                String::from_utf8(server_id.clone()).unwrap_or_default(),
            );
        }

        if let Some(version) = proto_msg.version {
            DnstapParser::insert(
                event,
                &root,
                &DNSTAP_VALUE_PATHS.server_version,
                String::from_utf8(version).unwrap_or_default(),
            );
        }

        if let Some(extra) = proto_msg.extra {
            DnstapParser::insert(
                event,
                &root,
                &DNSTAP_VALUE_PATHS.extra,
                String::from_utf8(extra).unwrap_or_default(),
            );
        }

        let dnstap_data_type_id: i32 = proto_msg.r#type;
        let mut need_raw_data = false;
        DnstapParser::insert(
            event,
            &root,
            &DNSTAP_VALUE_PATHS.data_type_id,
            dnstap_data_type_id,
        );

        if let Some(dnstap_data_type) = to_dnstap_data_type(dnstap_data_type_id) {
            DnstapParser::insert(
                event,
                &root,
                &DNSTAP_VALUE_PATHS.data_type,
                dnstap_data_type.clone(),
            );

            if dnstap_data_type == "Message" {
                if let Some(message) = proto_msg.message {
                    if let Err(err) =
                        DnstapParser::parse_dnstap_message(event, &root, message, parsing_options)
                    {
                        emit!(DnstapParseWarning { error: &err });
                        need_raw_data = true;
                        DnstapParser::insert(
                            event,
                            &root,
                            &DNSTAP_VALUE_PATHS.error,
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
            DnstapParser::insert(
                event,
                &root,
                &DNSTAP_VALUE_PATHS.raw_data,
                BASE64_STANDARD.encode(&frame),
            );
        }

        Ok(())
    }

    fn parse_dnstap_message<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        dnstap_message: DnstapMessage,
        parsing_options: DnsParserOptions,
    ) -> Result<()> {
        if let Some(socket_family) = dnstap_message.socket_family {
            DnstapParser::parse_dnstap_message_socket_family(
                event,
                prefix.clone(),
                socket_family,
                &dnstap_message,
            )?;
        }

        if let Some(query_zone) = dnstap_message.query_zone.as_ref() {
            let mut decoder: BinDecoder = BinDecoder::new(query_zone);
            match Name::read(&mut decoder) {
                Ok(raw_data) => {
                    DnstapParser::insert(
                        event,
                        prefix.clone(),
                        &DNSTAP_VALUE_PATHS.query_zone,
                        raw_data.to_utf8(),
                    );
                }
                Err(error) => return Err(Error::from(error.to_string())),
            }
        }

        let dnstap_message_type_id = dnstap_message.r#type;
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.message_type_id,
            dnstap_message_type_id,
        );

        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.message_type,
            to_dnstap_message_type(dnstap_message_type_id),
        );

        if let Some(query_time_sec) = dnstap_message.query_time_sec {
            DnstapParser::parse_dnstap_message_time(
                event,
                prefix.clone(),
                query_time_sec,
                dnstap_message.query_time_nsec,
                dnstap_message_type_id,
                dnstap_message.query_message.as_ref(),
                &DNSTAP_MESSAGE_REQUEST_TYPE_IDS,
            );
        }

        if let Some(response_time_sec) = dnstap_message.response_time_sec {
            DnstapParser::parse_dnstap_message_time(
                event,
                prefix.clone(),
                response_time_sec,
                dnstap_message.response_time_nsec,
                dnstap_message_type_id,
                dnstap_message.response_message.as_ref(),
                &DNSTAP_MESSAGE_RESPONSE_TYPE_IDS,
            );
        }

        DnstapParser::parse_dnstap_message_type(
            event,
            prefix.clone(),
            dnstap_message_type_id,
            dnstap_message,
            parsing_options,
        )?;

        Ok(())
    }

    fn parse_dnstap_message_type<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        dnstap_message_type_id: i32,
        dnstap_message: DnstapMessage,
        parsing_options: DnsParserOptions,
    ) -> Result<()> {
        match dnstap_message_type_id {
            1..=12 => {
                if let Some(query_message) = dnstap_message.query_message {
                    let mut query_message_parser =
                        DnsMessageParser::with_options(query_message, parsing_options.clone());
                    if let Err(error) = DnstapParser::parse_dns_query_message(
                        event,
                        prefix.concat(&DNSTAP_VALUE_PATHS.request_message),
                        &mut query_message_parser,
                    ) {
                        DnstapParser::log_raw_dns_message(
                            event,
                            prefix.concat(&DNSTAP_VALUE_PATHS.request_message),
                            query_message_parser.raw_message(),
                        );

                        return Err(error);
                    };
                }

                if let Some(response_message) = dnstap_message.response_message {
                    let mut response_message_parser =
                        DnsMessageParser::with_options(response_message, parsing_options);
                    if let Err(error) = DnstapParser::parse_dns_query_message(
                        event,
                        prefix.concat(&DNSTAP_VALUE_PATHS.response_message),
                        &mut response_message_parser,
                    ) {
                        DnstapParser::log_raw_dns_message(
                            event,
                            prefix.concat(&DNSTAP_VALUE_PATHS.response_message),
                            response_message_parser.raw_message(),
                        );

                        return Err(error);
                    };
                }
            }
            13 | 14 => {
                if let Some(update_request_message) = dnstap_message.query_message {
                    let mut update_request_message_parser = DnsMessageParser::with_options(
                        update_request_message,
                        parsing_options.clone(),
                    );
                    if let Err(error) = DnstapParser::parse_dns_update_message(
                        event,
                        &DNSTAP_VALUE_PATHS.request_message,
                        &mut update_request_message_parser,
                    ) {
                        DnstapParser::log_raw_dns_message(
                            event,
                            &DNSTAP_VALUE_PATHS.request_message,
                            update_request_message_parser.raw_message(),
                        );

                        return Err(error);
                    };
                }

                if let Some(update_response_message) = dnstap_message.response_message {
                    let mut update_response_message_parser =
                        DnsMessageParser::with_options(update_response_message, parsing_options);
                    if let Err(error) = DnstapParser::parse_dns_update_message(
                        event,
                        &DNSTAP_VALUE_PATHS.response_message,
                        &mut update_response_message_parser,
                    ) {
                        DnstapParser::log_raw_dns_message(
                            event,
                            &DNSTAP_VALUE_PATHS.response_message,
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

    fn parse_dnstap_message_time<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        time_sec: u64,
        time_nsec: Option<u32>,
        dnstap_message_type_id: i32,
        message: Option<&Vec<u8>>,
        type_ids: &HashSet<i32>,
    ) {
        let (time_in_nanosec, query_time_nsec) = match time_nsec {
            Some(nsec) => (time_sec as i64 * 1_000_000_000_i64 + nsec as i64, nsec),
            None => (time_sec as i64 * 1_000_000_000_i64, 0),
        };

        if type_ids.contains(&dnstap_message_type_id) {
            DnstapParser::log_time(event, prefix.clone(), time_in_nanosec, "ns");

            let timestamp = Utc
                .timestamp_opt(time_sec.try_into().unwrap(), query_time_nsec)
                .single()
                .expect("invalid timestamp");
            if let Some(timestamp_key) = log_schema().timestamp_key() {
                DnstapParser::insert(event, prefix.clone(), timestamp_key, timestamp);
            }
        }

        if message.is_none() {
            DnstapParser::log_time(
                event,
                prefix.concat(&DNSTAP_VALUE_PATHS.request_message),
                time_in_nanosec,
                "ns",
            );
        }
    }

    fn parse_dnstap_message_socket_family<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        socket_family: i32,
        dnstap_message: &DnstapMessage,
    ) -> Result<()> {
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.socket_family,
            to_socket_family_name(socket_family)?.to_string(),
        );

        if let Some(socket_protocol) = dnstap_message.socket_protocol {
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.socket_protocol,
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

            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.query_address,
                source_address.to_string(),
            );
        }

        if let Some(query_port) = dnstap_message.query_port {
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.query_port,
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

            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.response_address,
                response_addr.to_string(),
            );
        }

        Ok(if let Some(response_port) = dnstap_message.response_port {
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.response_port,
                response_port,
            );
        })
    }

    fn log_time<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        time: i64,
        time_precision: &str,
    ) {
        DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.time, time);
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.time_precision,
            time_precision.to_string(),
        );
    }

    fn log_raw_dns_message<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        raw_dns_message: &[u8],
    ) {
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.raw_data,
            BASE64_STANDARD.encode(raw_dns_message),
        );
    }

    fn parse_dns_query_message<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        dns_message_parser: &mut DnsMessageParser,
    ) -> Result<()> {
        let msg = dns_message_parser.parse_as_query_message()?;

        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.response_code,
            msg.response_code,
        );

        if let Some(response) = msg.response {
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.response,
                response.to_string(),
            );
        }

        DnstapParser::log_dns_query_message_header(
            event,
            prefix.concat(&DNSTAP_VALUE_PATHS.header),
            &msg.header,
        );

        DnstapParser::log_dns_query_message_query_section(
            event,
            prefix.concat(&DNSTAP_VALUE_PATHS.question_section),
            &msg.question_section,
        );

        DnstapParser::log_dns_message_record_section(
            event,
            prefix.concat(&DNSTAP_VALUE_PATHS.answer_section),
            &msg.answer_section,
        );

        DnstapParser::log_dns_message_record_section(
            event,
            prefix.concat(&DNSTAP_VALUE_PATHS.authority_section),
            &msg.authority_section,
        );

        DnstapParser::log_dns_message_record_section(
            event,
            prefix.concat(&DNSTAP_VALUE_PATHS.additional_section),
            &msg.additional_section,
        );

        DnstapParser::log_edns(
            event,
            prefix.concat(&DNSTAP_VALUE_PATHS.opt_pseudo_section),
            &msg.opt_pseudo_section,
        );

        Ok(())
    }

    fn log_dns_query_message_header<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        header: &QueryHeader,
    ) {
        DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.id, header.id);
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.opcode,
            header.opcode,
        );
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.rcode,
            u16::from(header.rcode),
        );
        DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.qr, header.qr);
        DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.aa, header.aa);
        DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.tc, header.tc);
        DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.rd, header.rd);
        DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.ra, header.ra);
        DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.ad, header.ad);
        DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.cd, header.cd);
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.question_count,
            header.question_count,
        );
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.answer_count,
            header.answer_count,
        );
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.authority_count,
            header.authority_count,
        );
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.ar_count,
            header.additional_count,
        );
    }

    fn log_dns_query_message_query_section<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        questions: &[QueryQuestion],
    ) {
        for (i, query) in questions.iter().enumerate() {
            let index_segment = path!(i as isize);
            DnstapParser::log_dns_query_question(event, prefix.concat(index_segment), query);
        }
    }

    fn log_dns_query_question<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        question: &QueryQuestion,
    ) {
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.domain_name,
            question.name.clone(),
        );
        if let Some(record_type) = question.record_type.clone() {
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.question_type,
                record_type,
            );
        }
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.question_type_id,
            question.record_type_id,
        );
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.class,
            question.class.clone(),
        );
    }

    fn parse_dns_update_message<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        dns_message_parser: &mut DnsMessageParser,
    ) -> Result<()> {
        let msg = dns_message_parser.parse_as_update_message()?;

        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.response_code,
            msg.response_code,
        );

        if let Some(response) = msg.response {
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.response,
                response.to_string(),
            );
        }

        DnstapParser::log_dns_update_message_header(
            event,
            prefix.concat(&DNSTAP_VALUE_PATHS.header),
            &msg.header,
        );

        DnstapParser::log_dns_update_message_zone_section(
            event,
            prefix.concat(&DNSTAP_VALUE_PATHS.zone_section),
            &msg.zone_to_update,
        );

        DnstapParser::log_dns_message_record_section(
            event,
            prefix.concat(&DNSTAP_VALUE_PATHS.prerequisite_section),
            &msg.prerequisite_section,
        );

        DnstapParser::log_dns_message_record_section(
            event,
            prefix.concat(&DNSTAP_VALUE_PATHS.update_section),
            &msg.update_section,
        );

        DnstapParser::log_dns_message_record_section(
            event,
            prefix.concat(&DNSTAP_VALUE_PATHS.additional_section),
            &msg.additional_section,
        );

        Ok(())
    }

    fn log_dns_update_message_header<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        header: &UpdateHeader,
    ) {
        DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.id, header.id);

        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.opcode,
            header.opcode,
        );

        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.rcode,
            u16::from(header.rcode),
        );

        DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.qr, header.qr);

        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.zone_count,
            header.zone_count,
        );

        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.prerequisite_count,
            header.prerequisite_count,
        );

        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.update_count,
            header.update_count,
        );

        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.ad_count,
            header.additional_count,
        );
    }

    fn log_dns_update_message_zone_section<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        zone: &ZoneInfo,
    ) {
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.zone_name,
            zone.name.clone(),
        );
        if let Some(zone_type) = zone.zone_type.clone() {
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.zone_type,
                zone_type,
            );
        }
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.zone_type_id,
            zone.zone_type_id,
        );
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.zone_class,
            zone.class.clone(),
        );
    }

    fn log_edns<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        opt_section: &Option<OptPseudoSection>,
    ) {
        if let Some(edns) = opt_section {
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.extended_rcode,
                edns.extended_rcode,
            );
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.version,
                edns.version,
            );
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.do_flag,
                edns.dnssec_ok,
            );
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.udp_max_payload_size,
                edns.udp_max_payload_size,
            );
            DnstapParser::log_edns_ede(event, prefix.concat(&DNSTAP_VALUE_PATHS.ede), &edns.ede);
            DnstapParser::log_edns_options(
                event,
                prefix.concat(&DNSTAP_VALUE_PATHS.options),
                &edns.options,
            );
        }
    }

    fn log_edns_ede<'a>(event: &mut LogEvent, prefix: impl ValuePath<'a>, options: &[EDE]) {
        options.iter().enumerate().for_each(|(i, entry)| {
            let index_segment = path!(i as isize);
            DnstapParser::log_edns_ede_entry(event, prefix.concat(index_segment), entry);
        });
    }

    fn log_edns_ede_entry<'a>(event: &mut LogEvent, prefix: impl ValuePath<'a>, entry: &EDE) {
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.info_code,
            entry.info_code(),
        );
        if let Some(purpose) = entry.purpose() {
            DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.purpose, purpose);
        }
        if let Some(extra_text) = entry.extra_text() {
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.extra_text,
                extra_text,
            );
        }
    }

    fn log_edns_options<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        options: &[EdnsOptionEntry],
    ) {
        options.iter().enumerate().for_each(|(i, opt)| {
            let index_segment = path!(i as isize);
            DnstapParser::log_edns_opt(event, prefix.concat(index_segment), opt);
        });
    }

    fn log_edns_opt<'a>(event: &mut LogEvent, prefix: impl ValuePath<'a>, opt: &EdnsOptionEntry) {
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.opt_code,
            opt.opt_code,
        );
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.opt_name,
            opt.opt_name.clone(),
        );
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.opt_data,
            opt.opt_data.clone(),
        );
    }

    fn log_dns_message_record_section<'a>(
        event: &mut LogEvent,
        prefix: impl ValuePath<'a>,
        records: &[DnsRecord],
    ) {
        for (i, record) in records.iter().enumerate() {
            let index_segment = path!(i as isize);
            DnstapParser::log_dns_record(event, prefix.concat(index_segment), record);
        }
    }

    fn log_dns_record<'a>(event: &mut LogEvent, prefix: impl ValuePath<'a>, record: &DnsRecord) {
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.domain_name,
            record.name.clone(),
        );
        if let Some(record_type) = record.record_type.clone() {
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.record_type,
                record_type,
            );
        }
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.record_type_id,
            record.record_type_id,
        );
        DnstapParser::insert(event, prefix.clone(), &DNSTAP_VALUE_PATHS.ttl, record.ttl);
        DnstapParser::insert(
            event,
            prefix.clone(),
            &DNSTAP_VALUE_PATHS.class,
            record.class.clone(),
        );
        if let Some(rdata) = &record.rdata {
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.rdata,
                rdata.to_string(),
            );
        };
        if let Some(rdata_bytes) = &record.rdata_bytes {
            DnstapParser::insert(
                event,
                prefix.clone(),
                &DNSTAP_VALUE_PATHS.rdata_bytes,
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
    use super::*;
    use crate::event::Value;
    use chrono::DateTime;
    use dnsmsg_parser::dns_message_parser::DnsParserOptions;
    use std::collections::BTreeMap;

    #[test]
    fn test_parse_dnstap_data_with_query_message() {
        let mut log_event = LogEvent::default();
        let raw_dnstap_data = "ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zcnoIAxACGAEiEAAAAAAAAA\
        AAAAAAAAAAAAAqECABBQJwlAAAAAAAAAAAADAw8+0CODVA7+zq9wVNMU3WNlI2kwIAAAABAAAAAAABCWZhY2Vib29rMQNjb\
        20AAAEAAQAAKQIAAACAAAAMAAoACOxjCAG9zVgzWgUDY29tAHgB";
        let dnstap_data = BASE64_STANDARD
            .decode(raw_dnstap_data)
            .expect("Invalid base64 encoded data.");
        let parse_result = DnstapParser::parse(
            &mut log_event,
            Bytes::from(dnstap_data),
            DnsParserOptions::default(),
        );
        assert!(parse_result.is_ok());

        let expected_map: BTreeMap<&str, Value> = BTreeMap::from([
            ("dataType", Value::Bytes(Bytes::from("Message"))),
            ("dataTypeId", Value::Integer(1)),
            ("messageType", Value::Bytes(Bytes::from("ResolverQuery"))),
            ("messageTypeId", Value::Integer(3)),
            ("queryZone", Value::Bytes(Bytes::from("com."))),
            ("requestData.fullRcode", Value::Integer(0)),
            ("requestData.header.aa", Value::Boolean(false)),
            ("requestData.header.ad", Value::Boolean(false)),
            ("requestData.header.anCount", Value::Integer(0)),
            ("requestData.header.arCount", Value::Integer(1)),
            ("requestData.header.cd", Value::Boolean(false)),
            ("requestData.header.id", Value::Integer(37634)),
            ("requestData.header.nsCount", Value::Integer(0)),
            ("requestData.header.opcode", Value::Integer(0)),
            ("requestData.header.qdCount", Value::Integer(1)),
            ("requestData.header.qr", Value::Integer(0)),
            ("requestData.header.ra", Value::Boolean(false)),
            ("requestData.header.rcode", Value::Integer(0)),
            ("requestData.header.rd", Value::Boolean(false)),
            ("requestData.header.tc", Value::Boolean(false)),
            ("requestData.opt.do", Value::Boolean(true)),
            ("requestData.opt.ednsVersion", Value::Integer(0)),
            ("requestData.opt.extendedRcode", Value::Integer(0)),
            ("requestData.opt.options[0].optCode", Value::Integer(10)),
            (
                "requestData.opt.options[0].optName",
                Value::Bytes(Bytes::from("Cookie")),
            ),
            (
                "requestData.opt.options[0].optValue",
                Value::Bytes(Bytes::from("7GMIAb3NWDM=")),
            ),
            ("requestData.opt.udpPayloadSize", Value::Integer(512)),
            (
                "requestData.question[0].class",
                Value::Bytes(Bytes::from("IN")),
            ),
            (
                "requestData.question[0].domainName",
                Value::Bytes(Bytes::from("facebook1.com.")),
            ),
            (
                "requestData.question[0].questionType",
                Value::Bytes(Bytes::from("A")),
            ),
            ("requestData.question[0].questionTypeId", Value::Integer(1)),
            (
                "requestData.rcodeName",
                Value::Bytes(Bytes::from("NoError")),
            ),
            (
                "responseAddress",
                Value::Bytes(Bytes::from("2001:502:7094::30")),
            ),
            ("responsePort", Value::Integer(53)),
            (
                "serverId",
                Value::Bytes(Bytes::from("james-Virtual-Machine")),
            ),
            ("serverVersion", Value::Bytes(Bytes::from("BIND 9.16.3"))),
            ("socketFamily", Value::Bytes(Bytes::from("INET6"))),
            ("socketProtocol", Value::Bytes(Bytes::from("UDP"))),
            ("sourceAddress", Value::Bytes(Bytes::from("::"))),
            ("sourcePort", Value::Integer(46835)),
            ("time", Value::Integer(1_593_489_007_920_014_129)),
            ("timePrecision", Value::Bytes(Bytes::from("ns"))),
            (
                "timestamp",
                Value::Timestamp(
                    Utc.from_utc_datetime(
                        &DateTime::parse_from_rfc3339("2020-06-30T03:50:07.920014129Z")
                            .unwrap()
                            .naive_utc(),
                    ),
                ),
            ),
        ]);

        // The maps need to contain identical keys and values.
        for (exp_key, exp_value) in expected_map {
            let value = log_event.get(exp_key).unwrap();
            assert_eq!(*value, exp_value);
        }
    }

    #[test]
    fn test_parse_dnstap_data_lowercase_hostnames() {
        let mut log_event = LogEvent::default();
        let mut lowercase_log_event = LogEvent::default();
        let raw_dnstap_data = "Cgw2NzNiNWZiZWI5MmESMkJJTkQgOS4xOC4yMS0xK3VidW50dTIyLjA0LjErZGViLnN1cnkub3JnKzEtVWJ1bnR1cqkBCAYQARgBIgQKWQUeKgQKWQUqMMitAjg1YLXQp68GbZ9tBw9ygwGInoGAAAEABAAAAAEGVmVjdG9yA0RldgAAAQABwAwAAQABAAAAPAAEEvVWOMAMAAEAAQAAADwABBL1VnnADAABAAEAAAA8AAQS9VYSwAwAAQABAAAAPAAEEvVWWQAAKQTQAAAAAAAcAAoAGERDbSN8uKngAQAAAGXp6DXs0fbpv0n9F3gB";
        let dnstap_data = BASE64_STANDARD
            .decode(raw_dnstap_data)
            .expect("Invalid base64 encoded data.");
        let parse_result = DnstapParser::parse(
            &mut lowercase_log_event,
            Bytes::from(dnstap_data.clone()),
            DnsParserOptions {
                lowercase_hostnames: true,
            },
        );
        let no_lowercase_result = DnstapParser::parse(
            &mut log_event,
            Bytes::from(dnstap_data),
            DnsParserOptions::default(),
        );
        assert!(parse_result.is_ok());
        assert!(no_lowercase_result.is_ok());

        let no_lowercase_expected: BTreeMap<&str, Value> = BTreeMap::from([
            ("dataType", Value::Bytes(Bytes::from("Message"))),
            ("dataTypeId", Value::Integer(1)),
            (
                "responseData.answers[0].domainName",
                Value::Bytes(Bytes::from("Vector.Dev.")),
            ),
            (
                "responseData.question[0].domainName",
                Value::Bytes(Bytes::from("Vector.Dev.")),
            ),
        ]);
        let expected_map: BTreeMap<&str, Value> = BTreeMap::from([
            ("dataType", Value::Bytes(Bytes::from("Message"))),
            ("dataTypeId", Value::Integer(1)),
            (
                "responseData.answers[0].domainName",
                Value::Bytes(Bytes::from("vector.dev.")),
            ),
            (
                "responseData.question[0].domainName",
                Value::Bytes(Bytes::from("vector.dev.")),
            ),
        ]);

        // The maps need to contain identical keys and values.
        for (exp_key, exp_value) in no_lowercase_expected {
            let value = log_event.get(exp_key).unwrap();
            assert_eq!(*value, exp_value);
        }
        for (exp_key, exp_value) in expected_map {
            let value = lowercase_log_event.get(exp_key).unwrap();
            assert_eq!(*value, exp_value);
        }
    }

    #[test]
    fn test_parse_dnstap_data_with_ede_options() {
        let mut log_event = LogEvent::default();
        let raw_dnstap_data = "ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zGgBy5wEIAxACGAEiEAAAAAAAAAAAAAAAAAAAAAAqECABBQJwlAAAAAAAAAAAADAw8+0CODVA7+zq9wVNMU3WNlI2kwIAAAABAAAAAAABCWZhY2Vib29rMQNjb20AAAEAAQAAKQIAAACAAAAMAAoACOxjCAG9zVgzWgUDY29tAGAAbQAAAAByZLM4AAAAAQAAAAAAAQJoNQdleGFtcGxlA2NvbQAABgABAAApBNABAUAAADkADwA1AAlubyBTRVAgbWF0Y2hpbmcgdGhlIERTIGZvdW5kIGZvciBkbnNzZWMtZmFpbGVkLm9yZy54AQ==";
        let dnstap_data = BASE64_STANDARD
            .decode(raw_dnstap_data)
            .expect("Invalid base64 encoded data.");
        let parse_result = DnstapParser::parse(
            &mut log_event,
            Bytes::from(dnstap_data),
            DnsParserOptions::default(),
        );
        assert!(parse_result.is_ok());

        let expected_map: BTreeMap<&str, Value> = BTreeMap::from([
            ("responseData.opt.ede[0].infoCode", Value::Integer(9)),
            (
                "responseData.opt.ede[0].purpose",
                Value::Bytes(Bytes::from("DNSKEY Missing")),
            ),
            (
                "responseData.opt.ede[0].extraText",
                Value::Bytes(Bytes::from(
                    "no SEP matching the DS found for dnssec-failed.org.",
                )),
            ),
        ]);

        // The maps need to contain identical keys and values.
        for (exp_key, exp_value) in expected_map {
            let value = log_event.get(exp_key).unwrap();
            assert_eq!(*value, exp_value);
        }
    }

    #[test]
    fn test_parse_dnstap_data_with_update_message() {
        let mut log_event = LogEvent::default();
        let raw_dnstap_data = "ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zcmsIDhABGAEiBH8AAA\
        EqBH8AAAEwrG44AEC+iu73BU14gfofUh1wi6gAAAEAAAAAAAAHZXhhbXBsZQNjb20AAAYAAWC+iu73BW0agDwvch1wi6gAA\
        AEAAAAAAAAHZXhhbXBsZQNjb20AAAYAAXgB";
        let dnstap_data = BASE64_STANDARD
            .decode(raw_dnstap_data)
            .expect("Invalid base64 encoded data.");
        let parse_result = DnstapParser::parse(
            &mut log_event,
            Bytes::from(dnstap_data),
            DnsParserOptions::default(),
        );
        assert!(parse_result.is_ok());

        let expected_map: BTreeMap<&str, Value> = BTreeMap::from([
            ("dataType", Value::Bytes(Bytes::from("Message"))),
            ("dataTypeId", Value::Integer(1)),
            ("messageType", Value::Bytes(Bytes::from("UpdateResponse"))),
            ("messageTypeId", Value::Integer(14)),
            ("requestData.fullRcode", Value::Integer(0)),
            ("requestData.header.adCount", Value::Integer(0)),
            ("requestData.header.id", Value::Integer(28811)),
            ("requestData.header.opcode", Value::Integer(5)),
            ("requestData.header.prCount", Value::Integer(0)),
            ("requestData.header.qr", Value::Integer(1)),
            ("requestData.header.rcode", Value::Integer(0)),
            ("requestData.header.upCount", Value::Integer(0)),
            ("requestData.header.zoCount", Value::Integer(1)),
            (
                "requestData.rcodeName",
                Value::Bytes(Bytes::from("NoError")),
            ),
            ("requestData.zone.zClass", Value::Bytes(Bytes::from("IN"))),
            (
                "requestData.zone.zName",
                Value::Bytes(Bytes::from("example.com.")),
            ),
            ("requestData.zone.zType", Value::Bytes(Bytes::from("SOA"))),
            ("requestData.zone.zTypeId", Value::Integer(6)),
            ("responseAddress", Value::Bytes(Bytes::from("127.0.0.1"))),
            ("responseData.fullRcode", Value::Integer(0)),
            ("responseData.header.adCount", Value::Integer(0)),
            ("responseData.header.id", Value::Integer(28811)),
            ("responseData.header.opcode", Value::Integer(5)),
            ("responseData.header.prCount", Value::Integer(0)),
            ("responseData.header.qr", Value::Integer(1)),
            ("responseData.header.rcode", Value::Integer(0)),
            ("responseData.header.upCount", Value::Integer(0)),
            ("responseData.header.zoCount", Value::Integer(1)),
            (
                "responseData.rcodeName",
                Value::Bytes(Bytes::from("NoError")),
            ),
            ("responseData.zone.zClass", Value::Bytes(Bytes::from("IN"))),
            (
                "responseData.zone.zName",
                Value::Bytes(Bytes::from("example.com.")),
            ),
            ("responseData.zone.zType", Value::Bytes(Bytes::from("SOA"))),
            ("responseData.zone.zTypeId", Value::Integer(6)),
            ("responsePort", Value::Integer(0)),
            (
                "serverId",
                Value::Bytes(Bytes::from("james-Virtual-Machine")),
            ),
            ("serverVersion", Value::Bytes(Bytes::from("BIND 9.16.3"))),
            ("socketFamily", Value::Bytes(Bytes::from("INET"))),
            ("socketProtocol", Value::Bytes(Bytes::from("UDP"))),
            ("sourceAddress", Value::Bytes(Bytes::from("127.0.0.1"))),
            ("sourcePort", Value::Integer(14124)),
            ("time", Value::Integer(1_593_541_950_792_494_106)),
            ("timePrecision", Value::Bytes(Bytes::from("ns"))),
            (
                "timestamp",
                Value::Timestamp(
                    Utc.from_utc_datetime(
                        &DateTime::parse_from_rfc3339("2020-06-30T18:32:30.792494106Z")
                            .unwrap()
                            .naive_utc(),
                    ),
                ),
            ),
        ]);

        // The maps need to contain identical keys and values.
        for (exp_key, exp_value) in expected_map {
            let value = log_event.get(exp_key).unwrap();
            assert_eq!(*value, exp_value);
        }
    }

    #[test]
    fn test_parse_dnstap_data_with_invalid_data() {
        let mut log_event = LogEvent::default();
        let e = DnstapParser::parse(
            &mut log_event,
            Bytes::from(vec![1, 2, 3]),
            DnsParserOptions::default(),
        )
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
