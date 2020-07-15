use super::dns_message::{
    DnsQueryMessage, DnsRecord, DnsUpdateMessage, EdnsOptionEntry, OptPseudoSection, QueryHeader,
    QueryQuestion, UpdateHeader, ZoneInfo,
};
use snafu::Snafu;
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

#[derive(Debug, Snafu)]
pub enum DnsMessageParserError {
    #[snafu(display("Encountered error : {}", cause))]
    SimpleError { cause: String },
    #[snafu(display("Encountered error from TrustDns: {}", failure_source.to_string()))]
    TrustDnsError { failure_source: ProtoError },
}

pub fn parse_dns_query_message(
    raw_dns_message: &Vec<u8>,
) -> Result<DnsQueryMessage, DnsMessageParserError> {
    match TrustDnsMessage::from_vec(raw_dns_message) {
        Ok(msg) => {
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
                parse_dns_message_section(msg.answers())?,
                parse_dns_message_section(&msg.name_servers())?,
                parse_dns_message_section(&msg.additionals())?,
                edns_section,
            ))
        }
        Err(e) => Err(DnsMessageParserError::TrustDnsError { failure_source: e }),
    }
}

pub fn parse_dns_update_message(
    raw_dns_message: &Vec<u8>,
) -> Result<DnsUpdateMessage, DnsMessageParserError> {
    match TrustDnsMessage::from_vec(raw_dns_message) {
        Ok(msg) => {
            let header = parse_dns_update_message_header(&msg);
            let edns_section = parse_edns(&msg);
            let rcode_high = if let Some(edns) = edns_section.clone() {
                edns.extended_rcode
            } else {
                0
            };
            let response_code = (u16::from(rcode_high) << 4) | ((u16::from(header.rcode)) & 0x000F);

            Ok(DnsUpdateMessage::new(
                response_code,
                parse_response_code(response_code),
                header,
                parse_dns_update_message_zone_section(&msg)?,
                parse_dns_message_section(msg.answers())?,
                parse_dns_message_section(&msg.name_servers())?,
                parse_dns_message_section(&msg.additionals())?,
                edns_section,
            ))
        }
        Err(e) => Err(DnsMessageParserError::TrustDnsError { failure_source: e }),
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

fn parse_dns_update_message_header(dns_message: &TrustDnsMessage) -> UpdateHeader {
    UpdateHeader::new(
        dns_message.header().id(),
        dns_message.header().op_code() as u8,
        dns_message.header().response_code(),
        dns_message.header().message_type() as u8,
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

fn parse_dns_update_message_zone_section(
    dns_message: &TrustDnsMessage,
) -> Result<ZoneInfo, DnsMessageParserError> {
    let mut zones: Vec<ZoneInfo> = Vec::new();

    for query in dns_message.queries().iter() {
        zones.push(parse_dns_query_question(query)?.into());
    }

    if zones.len() != 1 {
        Err(DnsMessageParserError::SimpleError {
            cause: format!(
                "Unexpected number of records in update section: {}",
                zones.len()
            ),
        })
    } else {
        Ok(zones.get(0).unwrap().to_owned())
    }
}

fn parse_dns_message_section(records: &[Record]) -> Result<Vec<DnsRecord>, DnsMessageParserError> {
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
        RData::ANAME(name) => Ok((Some(name.to_string()), None)),
        RData::CNAME(name) => Ok((Some(name.to_string()), None)),
        RData::MX(mx) => {
            let srv_rdata = format!("{} {}", mx.preference(), mx.exchange().to_string(),);
            Ok((Some(srv_rdata), None))
        }
        RData::NULL(null) => match null.anything() {
            Some(raw_rdata) => Ok((Some(base64::encode(raw_rdata)), None)),
            None => Ok((Some(String::from("")), None)),
        },
        RData::NS(ns) => Ok((Some(ns.to_string()), None)),
        RData::OPENPGPKEY(key) => {
            if let Ok(key_string) = String::from_utf8(Vec::from(key.public_key())) {
                Ok((Some(format!("({})", &key_string)), None))
            } else {
                Err(DnsMessageParserError::SimpleError {
                    cause: String::from("Invalid OPENPGPKEY rdata"),
                })
            }
        }
        RData::PTR(ptr) => Ok((Some(ptr.to_string()), None)),
        RData::SOA(soa) => Ok((
            Some(format!(
                "{} {} ({} {} {} {} {})",
                soa.mname().to_string(),
                soa.rname().to_string(),
                soa.serial(),
                soa.refresh(),
                soa.retry(),
                soa.expire(),
                soa.minimum()
            )),
            None,
        )),
        RData::SRV(srv) => {
            let srv_rdata = format!(
                "{} {} {} {}",
                srv.priority(),
                srv.weight(),
                srv.port(),
                srv.target().to_string()
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
                None => Err(DnsMessageParserError::SimpleError {
                    cause: String::from("Empty HINFO rdata"),
                }),
            },

            _ => match rdata.anything() {
                Some(raw_rdata) => Ok((None, Some(raw_rdata.clone()))),
                None => Err(DnsMessageParserError::SimpleError {
                    cause: String::from("Empty rdata"),
                }),
            },
        },
        _ => Err(DnsMessageParserError::SimpleError {
            cause: format!("Unsupported rdata {:?}", rdata),
        }),
    }
}

fn parse_character_string(decoder: &mut BinDecoder) -> Result<String, DnsMessageParserError> {
    match decoder.read_u8() {
        Ok(raw_len) => {
            let len = raw_len.unverified() as usize;
            match decoder.read_slice(len) {
                Ok(raw_text) => match raw_text.verify_unwrap(|r| r.len() == len) {
                    Ok(verified_text) => Ok(String::from_utf8_lossy(verified_text).to_string()),
                    Err(raw_data) => Err(DnsMessageParserError::SimpleError {
                        cause: format!(
                            "Unexpected data length: expected {}, got {}. Raw data {}",
                            len,
                            raw_data.len(),
                            format_bytes_as_hex_string(&raw_data.to_vec())
                        ),
                    }),
                },
                Err(error) => Err(DnsMessageParserError::TrustDnsError {
                    failure_source: error,
                }),
            }
        }
        Err(error) => Err(DnsMessageParserError::TrustDnsError {
            failure_source: error,
        }),
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
        1 => Some(String::from("A")),
        2 => Some(String::from("NS")),
        3 => Some(String::from("MD")),
        4 => Some(String::from("MF")),
        5 => Some(String::from("CNAME")),
        6 => Some(String::from("SOA")),
        7 => Some(String::from("MB")),
        8 => Some(String::from("MG")),
        9 => Some(String::from("MR")),
        10 => Some(String::from("NULL")),
        11 => Some(String::from("WKS")),
        12 => Some(String::from("PTR")),
        13 => Some(String::from("HINFO")),
        14 => Some(String::from("MINFO")),
        15 => Some(String::from("MX")),
        16 => Some(String::from("TXT")),
        17 => Some(String::from("RP")),
        18 => Some(String::from("AFSDB")),
        19 => Some(String::from("X25")),
        20 => Some(String::from("ISDN")),
        21 => Some(String::from("RT")),
        22 => Some(String::from("NSAP")),
        23 => Some(String::from("NSAP-PTR")),
        24 => Some(String::from("SIG")),
        25 => Some(String::from("KEY")),
        26 => Some(String::from("PX")),
        27 => Some(String::from("GPOS")),
        28 => Some(String::from("AAAA")),
        29 => Some(String::from("LOC")),
        30 => Some(String::from("NXT")),
        31 => Some(String::from("EID")),
        32 => Some(String::from("NIMLOC")),
        33 => Some(String::from("SRV")),
        34 => Some(String::from("ATMA")),
        35 => Some(String::from("NAPTR")),
        36 => Some(String::from("KX")),
        37 => Some(String::from("CERT")),
        38 => Some(String::from("A6")),
        39 => Some(String::from("DNAME")),
        40 => Some(String::from("SINK")),
        41 => Some(String::from("OPT")),
        42 => Some(String::from("APL")),
        43 => Some(String::from("DS")),
        44 => Some(String::from("SSHFP")),
        45 => Some(String::from("IPSECKEY")),
        46 => Some(String::from("RRSIG")),
        47 => Some(String::from("NSEC")),
        48 => Some(String::from("DNSKEY")),
        49 => Some(String::from("DHCID")),
        50 => Some(String::from("NSEC3")),
        51 => Some(String::from("NSEC3PARAM")),
        52 => Some(String::from("TLSA")),
        53 => Some(String::from("SMIMEA")),
        55 => Some(String::from("HIP")),
        56 => Some(String::from("NINFO")),
        57 => Some(String::from("RKEY")),
        58 => Some(String::from("TALINK")),
        59 => Some(String::from("CDS")),
        60 => Some(String::from("CDNSKEY")),
        61 => Some(String::from("OPENPGPKEY")),
        62 => Some(String::from("CSYNC")),
        63 => Some(String::from("ZONEMD")),
        99 => Some(String::from("SPF")),
        100 => Some(String::from("UINFO")),
        101 => Some(String::from("UID")),
        102 => Some(String::from("GID")),
        103 => Some(String::from("UNSPEC")),
        104 => Some(String::from("NID")),
        105 => Some(String::from("L32")),
        106 => Some(String::from("L64")),
        107 => Some(String::from("LP")),
        108 => Some(String::from("EUI48")),
        109 => Some(String::from("EUI64")),
        249 => Some(String::from("TKEY")),
        250 => Some(String::from("TSIG")),
        251 => Some(String::from("IXFR")),
        252 => Some(String::from("AXFR")),
        253 => Some(String::from("MAILB")),
        254 => Some(String::from("MAILA")),
        255 => Some(String::from("ANY")),
        256 => Some(String::from("URI")),
        257 => Some(String::from("CAA")),
        258 => Some(String::from("AVC")),
        259 => Some(String::from("DOA")),
        260 => Some(String::from("AMTRELAY")),
        32768 => Some(String::from("TA")),
        32769 => Some(String::from("DLV")),

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
    use std::{
        net::{Ipv4Addr, Ipv6Addr},
        str::FromStr,
        time::Instant,
    };
    use trust_dns_proto::rr::{domain::Name, rdata::TXT};

    #[test]
    fn test_parse_dns_query_message() {
        let raw_dns_message = "szgAAAABAAAAAAAAAmg1B2V4YW1wbGUDY29tAAAGAAE=";
        if let Ok(raw_qeury_message) = base64::decode(raw_dns_message) {
            let parse_result = parse_dns_query_message(&raw_qeury_message);
            assert!(parse_result.is_ok());
            if let Some(message) = parse_result.ok() {
                assert_eq!(message.header.qr, 0);
                assert_eq!(message.question_section.len(), 1);
                assert_eq!(
                    &message.question_section.first().unwrap().name,
                    "h5.example.com."
                );
                assert_eq!(
                    &message
                        .question_section
                        .first()
                        .unwrap()
                        .record_type
                        .clone()
                        .unwrap(),
                    "SOA"
                );
            } else {
                error!("Message is not parsed");
            }
        } else {
            error!("Invalid base64 encoded data");
        }
    }

    #[test]
    fn test_parse_dns_query_message_with_invalid_data() {
        if let Err(err) = parse_dns_query_message(&vec![1, 2, 3]) {
            assert!(err.to_string().contains("unexpected end of input"));
            match err {
                DnsMessageParserError::TrustDnsError { failure_source: e } => {
                    assert!(e.to_string().contains("unexpected end of input"))
                }
                DnsMessageParserError::SimpleError { cause: e } => {
                    error!("Expected TrustDnsError, got {}", &e)
                }
            }
        } else {
            error!("Expected TrustDnsError");
        }
    }

    #[test]
    fn test_parse_dns_query_message_with_unsupported_rdata() {
        let raw_query_message_base64 = "S9iAAAABAAAADwAbAV8DY29tAAABAAHADgACAAEAAqMAABQBYQxndGxkLX\
        NlcnZlcnMDbmV0AMAOAAIAAQACowAABAFiwCXADgACAAEAAqMAAAQBY8AlwA4AAgABAAKjAAAEAWTAJcAOAAIAAQACowAABA\
        FlwCXADgACAAEAAqMAAAQBZsAlwA4AAgABAAKjAAAEAWfAJcAOAAIAAQACowAABAFowCXADgACAAEAAqMAAAQBacAlwA4AAg\
        ABAAKjAAAEAWrAJcAOAAIAAQACowAABAFrwCXADgACAAEAAqMAAAQBbMAlwA4AAgABAAKjAAAEAW3AJcAOACsAAQABUYAAJH\
        i9CALi08kW9t7qxzKU6CaPtYhQRKgz/FRZWI9KkYTPxBpXZsAOAC4AAQABUYABEwArCAEAAVGAXwVS0F70IUC/BwCNcUk9rv\
        6TfbDiupYh7mlNIozd6xvNJLF7rJYhXTtCmw0eonl5dDZ5MWsIy11VHRQlwDMLYD691Imn8Pc+FqPBeWxT2ooZn2PT6dFiMo\
        D9lRGid8In5x5x7xqDOC0yORGXGq1slMk1yB+SXYU5GSBGc455QZT4m/voD8WcG8KFQNmb/J8RwVOLcHveQl+7BMF2ol0Uzg\
        TinFFfG4icEw5lMySS9HCCezFVxU1BFBF6pMqNAa/725+1VyfHbN1OVw5I47+IIvtn+nw3tiCJQLPo7hWlenKUUqFpHHL7II\
        sS5Z+fY2EFjjoZdqKMBBvcbqZkhyHUYDf01xsDPGDx6g/YwCMAAQABAAKjAAAEwAUGHsBDAAEAAQACowAABMAhDh7AUwABAA\
        EAAqMAAATAGlwewGMAAQABAAKjAAAEwB9QHsBzAAEAAQACowAABMAMXh7AgwABAAEAAqMAAATAIzMewJMAAQABAAKjAAAEwC\
        pdHsCjAAEAAQACowAABMA2cB7AswABAAEAAqMAAATAK6wewMMAAQABAAKjAAAEwDBPHsDTAAEAAQACowAABMA0sh7A4wABAA\
        EAAqMAAATAKaIewPMAAQABAAKjAAAEwDdTHsAjABwAAQACowAAECABBQOoPgAAAAAAAAACADDAQwAcAAEAAqMAABAgAQUDIx\
        0AAAAAAAAAAgAwwFMAHAABAAKjAAAQIAEFA4PrAAAAAAAAAAAAMMBjABwAAQACowAAECABBQCFbgAAAAAAAAAAADDAcwAcAA\
        EAAqMAABAgAQUCHKEAAAAAAAAAAAAwwIMAHAABAAKjAAAQIAEFA9QUAAAAAAAAAAAAMMCTABwAAQACowAAECABBQPuowAAAA\
        AAAAAAADDAowAcAAEAAqMAABAgAQUCCMwAAAAAAAAAAAAwwLMAHAABAAKjAAAQIAEFAznBAAAAAAAAAAAAMMDDABwAAQACow\
        AAECABBQJwlAAAAAAAAAAAADDA0wAcAAEAAqMAABAgAQUDDS0AAAAAAAAAAAAwwOMAHAABAAKjAAAQIAEFANk3AAAAAAAAAA\
        AAMMDzABwAAQACowAAECABBQGx+QAAAAAAAAAAADAAACkFqgAAgAAAAA==";
        if let Ok(raw_query_message) = base64::decode(raw_query_message_base64) {
            if let Err(err) = parse_dns_query_message(&raw_query_message) {
                assert!(err.to_string().contains("Unsupported rdata"));
                match err {
                    DnsMessageParserError::TrustDnsError { failure_source: e } => {
                        error!("Expected TrustDnsError, got {}", &e)
                    }
                    DnsMessageParserError::SimpleError { cause: e } => {
                        assert!(e.to_string().contains("Unsupported rdata"))
                    }
                }
            } else {
                error!("Expected Error");
            }
        } else {
            error!("Invalid base64 encoded data");
        }
    }

    #[test]
    fn test_format_bytes_as_hex_string() {
        assert_eq!(
            "01.02.03.AB.CD.EF",
            &format_bytes_as_hex_string(&vec![1, 2, 3, 0xab, 0xcd, 0xef])
        );
    }

    #[test]
    fn test_parse_unknown_record_type() {
        assert_eq!("A", parse_unknown_record_type(1).unwrap());
        assert_eq!("ANY", parse_unknown_record_type(255).unwrap());
        assert!(parse_unknown_record_type(22222).is_none());
    }

    #[test]
    fn test_parse_dns_udpate_message_failure() {
        let raw_dns_message = "ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zcq0ICAQQARgBIgSs\
        FDetKgTAN1MeMMn/Ajg1QL6m6fcFTQLEKhVS+QMSSIIAAAEAAAAFAAUJZmFjZWJvb2sxA2NvbQAAAQABwAwAAgABAAKjA\
        AAPA25zMQhyZW50b25kY8AWwAwAAgABAAKjAAAGA25zMsAvIENLMFBPSk1HODc0TEpSRUY3RUZOODQzMFFWSVQ4QlNNwB\
        YAMgABAAFRgAAjAQEAAAAUZQGgwlcg7hVvbE45Y2s62gMS2SoAByIAAAAAApDATAAuAAEAAVGAALcAMggCAAFRgF8ACGF\
        e9r15m6QDY29tAFOih16McCzogcR6RZIu3kqZa27Bo1jtfzwzDENJIZItSCRuLqRO6oA90sCLItOEQv0skpQKtJQXmTZU\
        nqe3XK+1t/Op8G9cmeMXgCvynTJmm0WouSv+SuwBOjgqCaNuWpwbiIcaXY/NlId1lPpl8LJyTIRtFqGifW0FnYFe/Lzs3\
        pfZLoKMAG4/8Upqqv4F+Ij1oue1C6KWe0hn+beIKkIgN0pLMjVFTUhITThBMDFNTzBBRVFRTjdHMlVQSjI4NjfAFgAyAA\
        EAAVGAACIBAQAAABQ86ELf24DH1kAfgQ4dyyuf0+6y5wAGIAAAAAASwCsAAQABAAKjAAAEbD0TCsArAAEAAQACowAABKx\
        iwCLARgABAAEAAqMAAAQuprYzwEYAAQABAAKjAAAEXXMcaAAAKRAAAACAAAAAWgUDY29tAGC+pun3BW2/LUQYcvkDEkiC\
        AAABAAAABQAFCWZhY2Vib29rMQNjb20AAAEAAcAMAAIAAQACowAADwNuczEIcmVudG9uZGPAFsAMAAIAAQACowAABgNuc\
        zLALyBDSzBQT0pNRzg3NExKUkVGN0VGTjg0MzBRVklUOEJTTcAWADIAAQABUYAAIwEBAAAAFGUBoMJXIO4Vb2xOOWNrOt\
        oDEtkqAAciAAAAAAKQwEwALgABAAFRgAC3ADIIAgABUYBfAAhhXva9eZukA2NvbQBToodejHAs6IHEekWSLt5KmWtuwaN\
        Y7X88MwxDSSGSLUgkbi6kTuqAPdLAiyLThEL9LJKUCrSUF5k2VJ6nt1yvtbfzqfBvXJnjF4Ar8p0yZptFqLkr/krsATo4\
        KgmjblqcG4iHGl2PzZSHdZT6ZfCyckyEbRahon1tBZ2BXvy87N6X2S6CjABuP/FKaqr+BfiI9aLntQuilntIZ/m3iCpCI\
        DdKSzI1RU1ISE04QTAxTU8wQUVRUU43RzJVUEoyODY3wBYAMgABAAFRgAAiAQEAAAAUPOhC39uAx9ZAH4EOHcsrn9Pusu\
        cABiAAAAAAEsArAAEAAQACowAABGw9EwrAKwABAAEAAqMAAASsYsAiwEYAAQABAAKjAAAELqa2M8BGAAEAAQACowAABF1\
        zHGgAACkQAAAAgAAAAHgB";
        if let Ok(raw_update_message) = base64::decode(raw_dns_message) {
            assert!(parse_dns_update_message(&raw_update_message).is_err());
        } else {
            error!("Invalid base64 encoded data");
        }
    }

    #[test]
    fn test_parse_dns_udpate_message() {
        let raw_dns_message = "xjUoAAABAAAAAQAAB2V4YW1wbGUDY29tAAAGAAECaDXADAD/AP8AAAAAAAA=";
        if let Ok(raw_update_message) = base64::decode(raw_dns_message) {
            let parse_result = parse_dns_update_message(&raw_update_message);
            assert!(parse_result.is_ok());
            if let Some(message) = parse_result.ok() {
                assert_eq!(message.header.qr, 0);
                assert_eq!(message.update_section.len(), 1);
                assert_eq!(&message.update_section.first().unwrap().class, "ANY");
                assert_eq!(&message.zone_to_update.zone_type.clone().unwrap(), "SOA");
                assert_eq!(&message.zone_to_update.name, "example.com.");
            } else {
                error!("Message is not parsed");
            }
        } else {
            error!("Invalid base64 encoded data");
        }
    }

    #[test]
    fn test_format_rdata_for_a_type() {
        let rdata = RData::A(Ipv4Addr::from_str("1.2.3.4").unwrap());
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!("1.2.3.4", parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_aaaa_type() {
        let rdata = RData::AAAA(Ipv6Addr::from_str("2001::1234").unwrap());
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!("2001::1234", parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_cname_type() {
        let rdata = RData::CNAME(Name::from_str("www.example.com.").unwrap());
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!("www.example.com.", parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_txt_type() {
        let rdata = RData::TXT(TXT::new(vec![
            "abc\"def".to_string(),
            "gh\\i".to_string(),
            "".to_string(),
            "j".to_string(),
        ]));
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            // println!("{}", parsed.unwrap());
            assert_eq!(r#""abc\"def" "gh\\i" "" "j""#, parsed.unwrap());
        }
    }

    #[test]
    #[ignore]
    fn benchmark_parse_dns_query_message() {
        let raw_dns_message = "szgAAAABAAAAAAAAAmg1B2V4YW1wbGUDY29tAAAGAAE=";
        if let Ok(raw_qeury_message) = base64::decode(raw_dns_message) {
            let start = Instant::now();
            let num = 10_000;
            for _ in 0..num {
                let parse_result = parse_dns_query_message(&raw_qeury_message);
                assert!(parse_result.is_ok());
            }
            let time_taken = Instant::now().duration_since(start);
            println!(
                "Time taken to parse {} DNS query messages: {:#?}",
                num, time_taken
            );
        } else {
            error!("Invalid base64 encoded data");
        }
    }
}
