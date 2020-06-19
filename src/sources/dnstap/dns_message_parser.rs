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
    #[error("Encountered error from TrustDns: {0}")]
    TrustDnsError(ProtoError),
}

pub fn parse_dns_query_message(
    raw_dns_message: &Vec<u8>,
) -> Result<DnsQueryMessage, DnsMessageParserError> {
    match TrustDnsMessage::from_vec(raw_dns_message) {
        Ok(msg) => {
            // println!("Query: {:?}", msg);
            let header = parse_dns_query_message_header(&msg);
            let edns_section = parse_edns(&msg);
            let rcode_high = if let Some(edns) = edns_section.clone() {
                edns.extended_rcode
            } else {
                0
            };
            let response_code = (u16::from(rcode_high) << 4) | ((u16::from(header.rcode)) & 0x000F);

            Ok(DnsQueryMessage::new(
                response_code,
                parse_response_code(response_code),
                header,
                parse_dns_query_message_question_section(&msg)?,
                parse_dns_query_message_section(msg.answers())?,
                parse_dns_query_message_section(&msg.name_servers())?,
                parse_dns_query_message_section(&msg.additionals())?,
                edns_section,
            ))
        }
        Err(e) => Err(DnsMessageParserError::TrustDnsError(e)),
    }
}

fn parse_response_code(rcode: u16) -> Option<&'static str> {
    match rcode {
        0 => Some("NoError"), // 0    NoError    No Error                             [RFC1035]
        1 => Some("FormErr"), // 1    FormErr    Format Error                         [RFC1035]
        2 => Some("ServFail"), // 2    ServFail   Server Failure                       [RFC1035]
        3 => Some("NXDomain"), // 3    NXDomain   Non-Existent Domain                  [RFC1035]
        4 => Some("NotImp"),  // 4    NotImp     Not Implemented                      [RFC1035]
        5 => Some("Refused"), // 5    Refused    Query Refused                        [RFC1035]
        6 => Some("YXDomain"), // 6    YXDomain   Name Exists when it should not       [RFC2136][RFC6672]
        7 => Some("YXRRSet"),  // 7    YXRRSet    RR Set Exists when it should not     [RFC2136]
        8 => Some("NXRRSet"),  // 8    NXRRSet    RR Set that should exist does not    [RFC2136]
        9 => Some("NotAuth"),  // 9    NotAuth    Server Not Authoritative for zone    [RFC2136]
        10 => Some("NotZone"), // 10   NotZone    Name not contained in zone           [RFC2136]
        // backwards compat for 4 bit ResponseCodes so far.
        // 16    BADVERS    Bad OPT Version    [RFC6891]
        16 => Some("BADSIG"), // 16    BADSIG    TSIG Signature Failure               [RFC2845]
        17 => Some("BADKEY"), // 17    BADKEY    Key not recognized                   [RFC2845]
        18 => Some("BADTIME"), // 18    BADTIME   Signature out of time window         [RFC2845]
        19 => Some("BADMODE"), // 19    BADMODE   Bad TKEY Mode                        [RFC2930]
        20 => Some("BADNAME"), // 20    BADNAME   Duplicate key name                   [RFC2930]
        21 => Some("BADALG"), // 21    BADALG    Algorithm not supported              [RFC2930]
        22 => Some("BADTRUNC"), // 22    BADTRUNC  Bad Truncation                       [RFC4635]
        23 => Some("BADCOOKIE"), // 23    BADCOOKIE (TEMPORARY - registered 2015-07-26, expires 2016-07-26)    Bad/missing server cookie    [draft-ietf-dnsop-cookies]
        _ => None,
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
    let rdata = format_rdata(record.rdata())?;

    Ok(DnsRecord::new(
        record.name().to_string(),
        record.dns_class().to_string(),
        format_record_type(&record.record_type()),
        u16::from(record.record_type()),
        record.ttl(),
        rdata.0,
        rdata.1,
    ))
}

fn format_record_type(record_type: &RecordType) -> Option<String> {
    match record_type {
        RecordType::Unknown(code) => parse_unknown_record_type(*code),
        _ => Some(record_type.to_string()),
    }
}

fn format_rdata(rdata: &RData) -> Result<(Option<String>, Option<Vec<u8>>), DnsMessageParserError> {
    match rdata {
        RData::A(ip) => Ok((Some(ip.to_string()), None)),
        RData::AAAA(ip) => Ok((Some(ip.to_string()), None)),
        RData::CNAME(name) => Ok((Some(name.to_utf8()), None)),
        RData::SRV(srv) => {
            let srv_rdata = format!(
                "{} {} {} {}",
                srv.priority(),
                srv.weight(),
                srv.port(),
                srv.target().to_utf8()
            );
            Ok((Some(srv_rdata), None))
        }
        RData::TXT(txt) => {
            let txt_rdata = txt
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
                .join(" ");
            Ok((Some(txt_rdata), None))
        }
        RData::SOA(soa) => Ok((
            Some(format!(
                "{} {} ({} {} {} {} {})",
                soa.mname().to_utf8(),
                soa.rname().to_utf8(),
                soa.serial(),
                soa.refresh(),
                soa.retry(),
                soa.expire(),
                soa.minimum()
            )),
            None,
        )),
        RData::Unknown { code, rdata } => match code {
            13 => match rdata.anything() {
                Some(raw_rdata) => {
                    let mut decoder = BinDecoder::new(raw_rdata);
                    let cpu = parse_character_string(&mut decoder)?;
                    let os = parse_character_string(&mut decoder)?;
                    Ok((
                        Some(format!(
                            "\"{}\" \"{}\"",
                            escape_string_for_text_representation(cpu),
                            escape_string_for_text_representation(os)
                        )),
                        None,
                    ))
                }
                None => Err(DnsMessageParserError::SimpleError(String::from(
                    "Empty HINFO rdata",
                ))),
            },

            _ => match rdata.anything() {
                Some(raw_rdata) => Ok((None, Some(raw_rdata.clone()))),
                None => Err(DnsMessageParserError::SimpleError(String::from(
                    "Empty rdata",
                ))),
            },
        },
        _ => Err(DnsMessageParserError::SimpleError(String::from(
            "Unsupported rdata",
        ))),
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
                        format_bytes_as_hex_string(&raw_data.to_vec())
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

fn format_bytes_as_hex_string(bytes: &Vec<u8>) -> String {
    bytes
        .iter()
        .map(|e| format!("{:02X}", e))
        .collect::<Vec<String>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dns_query_message_with_invalid_data() {
        if let Err(err) = parse_dns_query_message(&vec![1, 2, 3]) {
            assert!(err.to_string().contains("unexpected end of input"));
            match err {
                DnsMessageParserError::TrustDnsError(e) => {
                    assert!(e.to_string().contains("unexpected end of input"))
                }
                DnsMessageParserError::SimpleError(e) => {
                    error!("Expected TrustDnsError, got {}", &e)
                }
            }
        } else {
            error!("Expected TrustDnsError");
        }
    }
}
