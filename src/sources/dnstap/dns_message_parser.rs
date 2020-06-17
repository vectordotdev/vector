extern crate base64;
use super::dns_message::{
    DnsQueryMessage, DnsRecord, EdnsOptionEntry, OptPseudoSection, QueryHeader, QueryQuestion,
};
use thiserror::Error as ThisError;
#[cfg(unix)]
use trust_dns_proto::{
    error::ProtoError,
    op::{message::Message as TrustDnsMessage, Edns, Query},
    rr::{
        dnssec::SupportedAlgorithms,
        rdata::opt::{EdnsCode, EdnsOption},
        record_data::RData,
        resource::Record,
        RecordType,
    },
    serialize::binary::BinDecoder,
};

#[derive(ThisError, Debug)]
pub enum DnsMessageParserError {
    #[error("Encountered error : {0}")]
    SimpleError(String),
    #[error("Encountered error from TrustDns")]
    TrustDnsError(ProtoError),
}

pub fn parse_dns_query_message(
    raw_dns_message: &Vec<u8>,
) -> Result<DnsQueryMessage, DnsMessageParserError> {
    match TrustDnsMessage::from_vec(raw_dns_message) {
        Ok(msg) => {
            println!("Query: {:?}", msg);

            Ok(DnsQueryMessage::new(
                parse_dns_query_message_header(&msg),
                parse_dns_query_message_question_section(&msg)?,
                parse_dns_query_message_section(msg.answers())?,
                parse_dns_query_message_section(&msg.name_servers())?,
                parse_dns_query_message_section(&msg.additionals())?,
                parse_edns(&msg),
            ))
        }
        Err(e) => Err(DnsMessageParserError::TrustDnsError(e)),
    }
}

fn parse_dns_query_message_header(dns_message: &TrustDnsMessage) -> QueryHeader {
    QueryHeader::new(
        dns_message.header().id(),
        dns_message.header().op_code() as u8,
        dns_message.header().response_code(),
        dns_message.header().message_type() as u8,
        dns_message.header().authoritative(),
        dns_message.header().truncated(),
        dns_message.header().recursion_desired(),
        dns_message.header().recursion_available(),
        dns_message.header().authentic_data(),
        dns_message.header().checking_disabled(),
        dns_message.header().query_count(),
        dns_message.header().answer_count(),
        dns_message.header().name_server_count(),
        dns_message.header().additional_count(),
    )
}

fn parse_dns_query_message_question_section(
    dns_message: &TrustDnsMessage,
) -> Result<Vec<QueryQuestion>, DnsMessageParserError> {
    let mut questions: Vec<QueryQuestion> = Vec::new();
    for query in dns_message.queries().iter() {
        questions.push(parse_dns_query_question(query)?);
    }
    Ok(questions)
}

fn parse_dns_query_message_section(
    records: &[Record],
) -> Result<Vec<DnsRecord>, DnsMessageParserError> {
    let mut answers: Vec<DnsRecord> = Vec::new();
    for record in records.iter() {
        answers.push(parse_dns_record(record)?);
    }
    Ok(answers)
}

fn parse_edns(dns_message: &TrustDnsMessage) -> Option<OptPseudoSection> {
    match dns_message.edns() {
        Some(edns) => Some(OptPseudoSection::new(
            edns.rcode_high(),
            edns.version(),
            edns.dnssec_ok(),
            edns.max_payload(),
            parse_edns_options(edns),
        )),

        None => None,
    }
}

fn parse_edns_options(edns: &Edns) -> Vec<EdnsOptionEntry> {
    edns.options()
        .options()
        .iter()
        .map(|(code, option)| match option {
            EdnsOption::DAU(algorithms)
            | EdnsOption::DHU(algorithms)
            | EdnsOption::N3U(algorithms) => parse_edns_opt_dnssec_algorithms(code, algorithms),
            EdnsOption::Unknown(_, opt_data) => parse_edns_opt(code, opt_data),
        })
        .collect()
}

fn parse_edns_opt_dnssec_algorithms(
    opt_code: &EdnsCode,
    algorithms: &SupportedAlgorithms,
) -> EdnsOptionEntry {
    let algorithm_names: Vec<String> = algorithms.iter().map(|alg| alg.to_string()).collect();
    EdnsOptionEntry::new(
        Into::<u16>::into(*opt_code),
        format!("{:?}", opt_code),
        algorithm_names.join(" "),
    )
}

fn parse_edns_opt(opt_code: &EdnsCode, opt_data: &Vec<u8>) -> EdnsOptionEntry {
    EdnsOptionEntry::new(
        Into::<u16>::into(*opt_code),
        format!("{:?}", opt_code),
        base64::encode(&opt_data),
    )
}

fn parse_dns_record(record: &Record) -> Result<DnsRecord, DnsMessageParserError> {
    Ok(DnsRecord::new(
        record.name().to_string(),
        record.dns_class().to_string(),
        format_record_type(&record.record_type()),
        u16::from(record.record_type()),
        record.ttl(),
        format_rdata(record.rdata())?,
    ))
}

fn format_record_type(record_type:&RecordType)->Option<String>{
    match record_type {
        RecordType::Unknown(code)=>parse_unknown_record_type(*code),
        _ => Some(record_type.to_string()),
    }
}

fn format_rdata(rdata: &RData) -> Result<String, DnsMessageParserError> {
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
                None => Err(DnsMessageParserError::SimpleError(String::from(
                    "Empty HINFO rdata",
                ))),
            },

            _ => match rdata.anything() {
                Some(raw_rdata) => Ok(base64::encode(raw_rdata)),
                None => Err(DnsMessageParserError::SimpleError(String::from(
                    "Empty rdata",
                ))),
            },
        },
        _ => Ok(String::from("unknown yet")),
    }
}

fn parse_character_string(decoder: &mut BinDecoder) -> Result<String, DnsMessageParserError> {
    match decoder.read_u8() {
        Ok(raw_len) => {
            let len = raw_len.unverified() as usize;
            match decoder.read_slice(len) {
                Ok(raw_text) => match raw_text.verify_unwrap(|r| r.len() == len) {
                    Ok(verified_text) => Ok(String::from_utf8_lossy(verified_text).to_string()),
                    Err(raw_data) => Err(DnsMessageParserError::SimpleError(format!(
                        "Unexpected data length: expected {}, got {}. Raw data {}",
                        len,
                        raw_data.len(),
                        base64::encode(&raw_data.to_vec())
                    ))),
                },
                Err(error) => Err(DnsMessageParserError::TrustDnsError(error)),
            }
        }
        Err(error) => Err(DnsMessageParserError::TrustDnsError(error)),
    }
}

fn escape_string_for_text_representation(original_string: String) -> String {
    original_string.replace("\\", "\\\\").replace("\"", "\\\"")
}

fn parse_dns_query_question(question: &Query) -> Result<QueryQuestion, DnsMessageParserError> {
    Ok(QueryQuestion::new(
        question.name().to_string(),
        question.query_class().to_string(),
        format_record_type(&question.query_type()),
        u16::from(question.query_type()),
    ))
}

fn parse_unknown_record_type(rtype: u16) -> Option<String> {
    match rtype {
        13 => Some(String::from("HINFO")),
        20 => Some(String::from("ISDN")),
        38 => Some(String::from("A6")),
        39 => Some(String::from("DNAME")),
        _ => None,
    }
}
