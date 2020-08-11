use super::dns_message::{
    DnsQueryMessage, DnsRecord, DnsUpdateMessage, EdnsOptionEntry, OptPseudoSection, QueryHeader,
    QueryQuestion, UpdateHeader, ZoneInfo,
};
use data_encoding::{BASE32HEX_NOPAD, BASE64, HEXUPPER};
use snafu::{ResultExt, Snafu};
use std::str::Utf8Error;
use trust_dns_proto::{
    error::ProtoError,
    op::{message::Message as TrustDnsMessage, Edns, Query},
    rr::{
        // rdata::caa::{Property, Value, KeyValue},
        dnssec::rdata::DNSSECRData,
        dnssec::SupportedAlgorithms,
        rdata::{
            opt::{EdnsCode, EdnsOption},
            NULL,
        },
        record_data::RData,
        resource::Record,
        Name,
        RecordType,
    },
    serialize::binary::{BinDecodable, BinDecoder},
};

#[derive(Debug, Snafu)]
pub enum DnsMessageParserError {
    #[snafu(display("Encountered error : {}", cause))]
    SimpleError { cause: String },
    #[snafu(display("Encountered error from TrustDns: {}", failure_source.to_string()))]
    TrustDnsError { failure_source: ProtoError },
    #[snafu(display("UTF8Error: {}", source))]
    Utf8ParsingError { source: Utf8Error },
}

pub struct DnsMessageParser {
    raw_message: Vec<u8>,
    raw_message_clone: Option<Vec<u8>>, // Used only for parsing compressed domain names
}

impl DnsMessageParser {
    pub fn new(raw_message: Vec<u8>) -> Self {
        DnsMessageParser {
            raw_message,
            raw_message_clone: None,
        }
    }

    pub fn raw_message(&self) -> &Vec<u8> {
        &self.raw_message
    }

    pub fn parse_as_query_message(&mut self) -> Result<DnsQueryMessage, DnsMessageParserError> {
        match TrustDnsMessage::from_vec(&self.raw_message) {
            Ok(msg) => {
                let header = parse_dns_query_message_header(&msg);
                let edns_section = parse_edns(&msg);
                let rcode_high = if let Some(edns) = edns_section.clone() {
                    edns.extended_rcode
                } else {
                    0
                };
                let response_code =
                    (u16::from(rcode_high) << 4) | ((u16::from(header.rcode)) & 0x000F);

                Ok(DnsQueryMessage::new(
                    response_code,
                    parse_response_code(response_code),
                    header,
                    self.parse_dns_query_message_question_section(&msg)?,
                    self.parse_dns_message_section(msg.answers())?,
                    self.parse_dns_message_section(&msg.name_servers())?,
                    self.parse_dns_message_section(&msg.additionals())?,
                    edns_section,
                ))
            }
            Err(e) => Err(DnsMessageParserError::TrustDnsError { failure_source: e }),
        }
    }

    pub fn parse_as_update_message(&mut self) -> Result<DnsUpdateMessage, DnsMessageParserError> {
        match TrustDnsMessage::from_vec(&self.raw_message) {
            Ok(msg) => {
                let header = parse_dns_update_message_header(&msg);
                let edns_section = parse_edns(&msg);
                let rcode_high = if let Some(edns) = edns_section.clone() {
                    edns.extended_rcode
                } else {
                    0
                };
                let response_code =
                    (u16::from(rcode_high) << 4) | ((u16::from(header.rcode)) & 0x000F);

                Ok(DnsUpdateMessage::new(
                    response_code,
                    parse_response_code(response_code),
                    header,
                    self.parse_dns_update_message_zone_section(&msg)?,
                    self.parse_dns_message_section(msg.answers())?,
                    self.parse_dns_message_section(&msg.name_servers())?,
                    self.parse_dns_message_section(&msg.additionals())?,
                    edns_section,
                ))
            }
            Err(e) => Err(DnsMessageParserError::TrustDnsError { failure_source: e }),
        }
    }

    fn parse_dns_query_message_question_section(
        &self,
        dns_message: &TrustDnsMessage,
    ) -> Result<Vec<QueryQuestion>, DnsMessageParserError> {
        let mut questions: Vec<QueryQuestion> = Vec::new();
        for query in dns_message.queries().iter() {
            questions.push(self.parse_dns_query_question(query)?);
        }
        Ok(questions)
    }

    fn parse_dns_query_question(
        &self,
        question: &Query,
    ) -> Result<QueryQuestion, DnsMessageParserError> {
        Ok(QueryQuestion::new(
            question.name().to_string(),
            question.query_class().to_string(),
            format_record_type(question.query_type()),
            u16::from(question.query_type()),
        ))
    }

    fn parse_dns_update_message_zone_section(
        &self,
        dns_message: &TrustDnsMessage,
    ) -> Result<ZoneInfo, DnsMessageParserError> {
        let mut zones: Vec<ZoneInfo> = Vec::new();

        for query in dns_message.queries().iter() {
            zones.push(self.parse_dns_query_question(query)?.into());
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

    fn parse_dns_message_section(
        &mut self,
        records: &[Record],
    ) -> Result<Vec<DnsRecord>, DnsMessageParserError> {
        let mut answers: Vec<DnsRecord> = Vec::new();
        for record in records.iter() {
            answers.push(self.parse_dns_record(record)?);
        }
        Ok(answers)
    }

    fn parse_dns_record(&mut self, record: &Record) -> Result<DnsRecord, DnsMessageParserError> {
        let record_data = match record.rdata() {
            RData::Unknown { code, rdata } => self.format_unknown_rdata(*code, rdata)?,
            _ => format_rdata(record.rdata())?,
        };

        Ok(DnsRecord::new(
            record.name().to_string(),
            record.dns_class().to_string(),
            format_record_type(record.record_type()),
            u16::from(record.record_type()),
            record.ttl(),
            record_data.0,
            record_data.1,
        ))
    }

    fn get_rdata_decoder_with_raw_message(&mut self, raw_rdata: &[u8]) -> BinDecoder {
        let (index, raw_message_clone_data) = if let Some(mut buf) = self.raw_message_clone.take() {
            let index = buf.len();
            buf.extend_from_slice(raw_rdata);
            (index, buf)
        } else {
            let mut buf = Vec::<u8>::with_capacity(self.raw_message.len() * 2);
            buf.extend(&self.raw_message);
            buf.extend_from_slice(raw_rdata);
            (self.raw_message.len(), buf)
        };
        self.raw_message_clone = Some(raw_message_clone_data);

        BinDecoder::new(self.raw_message_clone.as_ref().unwrap()).clone(index as u16)
    }

    fn format_unknown_rdata(
        &mut self,
        code: u16,
        rdata: &NULL,
    ) -> Result<(Option<String>, Option<Vec<u8>>), DnsMessageParserError> {
        match code {
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

            14 => match rdata.anything() {
                Some(raw_rdata) => {
                    let mut decoder = self.get_rdata_decoder_with_raw_message(raw_rdata);
                    let rmailbx = match Name::read(&mut decoder) {
                        Ok(data) => data,
                        Err(error) => {
                            return Err(DnsMessageParserError::SimpleError {
                                cause: error.to_string(),
                            })
                        }
                    };
                    let emailbx = match Name::read(&mut decoder) {
                        Ok(data) => data,
                        Err(error) => {
                            return Err(DnsMessageParserError::SimpleError {
                                cause: error.to_string(),
                            })
                        }
                    };
                    Ok((
                        Some(format!("{} {}", rmailbx.to_string(), emailbx.to_string())),
                        None,
                    ))
                }
                None => Err(DnsMessageParserError::SimpleError {
                    cause: String::from("Empty MINFO rdata"),
                }),
            },
            _ => match rdata.anything() {
                Some(raw_rdata) => Ok((None, Some(raw_rdata.clone()))),
                None => Err(DnsMessageParserError::SimpleError {
                    cause: String::from("Empty rdata"),
                }),
            },
        }
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
                "{} {} {} {} {} {} {}",
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
        // RData::CAA(caa) => {
        //     let caa_rdata = format!(
        //         "{} {} \"{}\"",
        //         caa.issuer_critical().to_string(),
        //         // caa.tag().as_str(),
        //         match caa.tag() {
        //             Property::Issue => "issue",
        //             Property::IssueWild => "issuewild",
        //             Property::Iodef => "iodef",
        //             Property::Unknown(ref property) => property,
        //         },
        //         match caa.value() {
        //             Value::Url(url) => { url.as_str().to_string()}
        //             Value::Issuer(option_name, vec_keyvalue) => {
        //                 let mut final_issuer = String::new();
        //                 if let Some(name) = option_name {
        //                     final_issuer.push_str(&name.to_utf8());
        //                     for keyvalue in vec_keyvalue.into_iter() {
        //                         final_issuer.push(';');
        //                         final_issuer.push_str(&keyvalue.key);
        //                         final_issuer.push_str("=");
        //                         final_issuer.push_str(&keyvalue.value);
        //                     }
        //                 }
        //                 //raise error ?
        //                 final_issuer
        //             }
        //             Value::Unknown(unknown) => std::str::from_utf8(unknown).context(Utf8ParsingError)?.to_string()

        //         }
        //     );
        //     Ok((Some(caa_rdata), None))
        // }
        RData::TLSA(tlsa) => {
            let tlsa_rdata = format!(
                "{} {} {} {}",
                u8::from(*tlsa.cert_usage()),
                u8::from(*tlsa.selector()),
                u8::from(*tlsa.matching()),
                HEXUPPER.encode(&tlsa.cert_data())
            );
            Ok((Some(tlsa_rdata), None))
        }
        RData::SSHFP(sshfp) => {
            let sshfp_rdata = format!(
                "{} {} {}",
                Into::<u8>::into(sshfp.algorithm()),
                Into::<u8>::into(sshfp.fingerprint_type()),
                HEXUPPER.encode(&sshfp.fingerprint())
            );
            Ok((Some(sshfp_rdata), None))
        }
        RData::NAPTR(naptr) => {
            let naptr_rdata = format!(
                r#"{} {} "{}" "{}" "{}" {}"#, // why the " escaping desnt work?
                naptr.order(),
                naptr.preference(),
                escape_string_for_text_representation(
                    std::str::from_utf8(naptr.flags())
                        .context(Utf8ParsingError)?
                        .to_string()
                ),
                escape_string_for_text_representation(
                    std::str::from_utf8(naptr.services())
                        .context(Utf8ParsingError)?
                        .to_string()
                ),
                escape_string_for_text_representation(
                    std::str::from_utf8(naptr.regexp())
                        .context(Utf8ParsingError)?
                        .to_string()
                ),
                naptr.replacement().to_utf8()
            );
            Ok((Some(naptr_rdata), None))
        }
        RData::DNSSEC(dnssec) => match dnssec {
            DNSSECRData::DS(ds) => {
                let ds_rdata = format!(
                    "{} {} {} {}",
                    ds.key_tag(),
                    u8::from(*ds.algorithm()),
                    u8::from(ds.digest_type()),
                    HEXUPPER.encode(ds.digest())
                );
                Ok((Some(ds_rdata), None))
            }
            DNSSECRData::DNSKEY(dnskey) => {
                let dnskey_rdata = format!(
                    "{} 3 {} {}",
                    {
                        if dnskey.revoke() {
                            String::from("0")
                        } else if dnskey.zone_key() && dnskey.secure_entry_point() {
                            String::from("257")
                        } else {
                            String::from("256")
                        }
                    },
                    u8::from(dnskey.algorithm()),
                    BASE64.encode(dnskey.public_key())
                );
                Ok((Some(dnskey_rdata), None))
            }
            DNSSECRData::NSEC(nsec) => {
                let nsec_rdata = format!(
                    "{} {}",
                    nsec.next_domain_name().to_string(),
                    nsec.type_bit_maps()
                        .iter()
                        .flat_map(|e| format_record_type(*e))
                        .collect::<Vec<String>>()
                        .join(" ")
                );
                Ok((Some(nsec_rdata), None))
            }
            DNSSECRData::NSEC3(nsec3) => {
                let nsec3_rdata = format!(
                    "{} {} {} {} {} {}",
                    u8::from(nsec3.hash_algorithm()),
                    nsec3.opt_out() as u8,
                    nsec3.iterations(),
                    HEXUPPER.encode(&nsec3.salt()),
                    BASE32HEX_NOPAD.encode(&nsec3.next_hashed_owner_name()),
                    nsec3
                        .type_bit_maps()
                        .iter()
                        .flat_map(|e| format_record_type(*e))
                        .collect::<Vec<String>>()
                        .join(" ")
                );
                Ok((Some(nsec3_rdata), None))
            }
            DNSSECRData::NSEC3PARAM(nsec3param) => {
                let nsec3param_rdata = format!(
                    "{} {} {} {}",
                    u8::from(nsec3param.hash_algorithm()),
                    nsec3param.opt_out() as u8,
                    nsec3param.iterations(),
                    HEXUPPER.encode(&nsec3param.salt()),
                );
                Ok((Some(nsec3param_rdata), None))
            }

            DNSSECRData::SIG(sig) => {
                let sig_rdata = format!(
                    "{} {} {} {} {} {} {} {} {}",
                    match format_record_type(sig.type_covered()) {
                        Some(record_type) => record_type,
                        None => String::from("Unknown record type"),
                    },
                    u8::from(sig.algorithm()),
                    sig.num_labels(),
                    sig.original_ttl(),
                    sig.sig_expiration(), // currently in epoch convert to human readable ?
                    sig.sig_inception(),  // currently in epoch convert to human readable ?
                    sig.key_tag(),
                    sig.signer_name().to_string(),
                    base64::encode(sig.sig())
                );
                Ok((Some(sig_rdata), None))
            }
            DNSSECRData::Unknown { code, rdata } => match rdata.anything() {
                Some(raw_rdata) => Ok((None, Some(raw_rdata.clone()))),
                None => Err(DnsMessageParserError::SimpleError {
                    cause: format!("Empty rdata with rcode {}", code),
                }),
            },
            _ => Err(DnsMessageParserError::SimpleError {
                cause: format!("Unsupported rdata {:?}", rdata),
            }),
        },
        _ => Err(DnsMessageParserError::SimpleError {
            cause: format!("Unsupported rdata {:?}", rdata),
        }),
    }
}

fn format_record_type(record_type: RecordType) -> Option<String> {
    match record_type {
        RecordType::Unknown(code) => parse_unknown_record_type(code),
        _ => Some(record_type.to_string()),
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
            | EdnsOption::N3U(algorithms) => parse_edns_opt_dnssec_algorithms(*code, *algorithms),
            EdnsOption::Unknown(_, opt_data) => parse_edns_opt(*code, opt_data),
        })
        .collect()
}

fn parse_edns_opt_dnssec_algorithms(
    opt_code: EdnsCode,
    algorithms: SupportedAlgorithms,
) -> EdnsOptionEntry {
    let algorithm_names: Vec<String> = algorithms.iter().map(|alg| alg.to_string()).collect();
    EdnsOptionEntry::new(
        Into::<u16>::into(opt_code),
        format!("{:?}", opt_code),
        algorithm_names.join(" "),
    )
}

fn parse_edns_opt(opt_code: EdnsCode, opt_data: &[u8]) -> EdnsOptionEntry {
    EdnsOptionEntry::new(
        Into::<u16>::into(opt_code),
        format!("{:?}", opt_code),
        base64::encode(&opt_data),
    )
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

fn format_bytes_as_hex_string(bytes: &[u8]) -> String {
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
    use trust_dns_proto::{
        rr::{
            dnssec::{
                rdata::{
                    dnskey::DNSKEY, ds::DS, nsec::NSEC, nsec3::NSEC3, nsec3param::NSEC3PARAM,
                    sig::SIG, DNSSECRData,
                },
                Algorithm as DNSSEC_Algorithm, DigestType, Nsec3HashAlgorithm,
            },
            domain::Name,
            rdata::{null, NAPTR, SSHFP, TLSA, TXT},
            rdata::{
                sshfp::{Algorithm, FingerprintType},
                tlsa::{CertUsage, Matching, Selector},
            },
        },
        serialize::binary::Restrict,
    };

    impl DnsMessageParser {
        pub fn raw_message_clone(&self) -> Option<&Vec<u8>> {
            self.raw_message_clone.as_ref()
        }
    }

    #[test]
    fn test_parse_as_query_message() {
        let raw_dns_message = "szgAAAABAAAAAAAAAmg1B2V4YW1wbGUDY29tAAAGAAE=";
        if let Ok(raw_qeury_message) = base64::decode(raw_dns_message) {
            let parse_result = DnsMessageParser::new(raw_qeury_message).parse_as_query_message();
            assert!(parse_result.is_ok());
            if let Ok(message) = parse_result {
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
    fn test_parse_as_query_message_with_invalid_data() {
        if let Err(err) = DnsMessageParser::new(vec![1, 2, 3]).parse_as_query_message() {
            assert!(err.to_string().contains("unexpected end of input"));
            match err {
                DnsMessageParserError::TrustDnsError { failure_source: e } => {
                    assert!(e.to_string().contains("unexpected end of input"))
                }
                DnsMessageParserError::SimpleError { cause: e } => {
                    error!("Expected TrustDnsError, got {}", &e)
                }
                _ => error!("{}", err),
            }
        } else {
            error!("Expected TrustDnsError");
        }
    }

    #[test]
    fn test_parse_as_query_message_with_unsupported_rdata() {
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
            if let Err(err) = DnsMessageParser::new(raw_query_message).parse_as_query_message() {
                assert!(err.to_string().contains("Unsupported rdata"));
                match err {
                    DnsMessageParserError::TrustDnsError { failure_source: e } => {
                        error!("Expected TrustDnsError, got {}", &e)
                    }
                    DnsMessageParserError::SimpleError { cause: e } => {
                        assert!(e.contains("Unsupported rdata"))
                    }
                    _ => error!("{}", err),
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
    fn test_parse_as_udpate_message_failure() {
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
            assert!(DnsMessageParser::new(raw_update_message)
                .parse_as_update_message()
                .is_err());
        } else {
            error!("Invalid base64 encoded data");
        }
    }

    #[test]
    fn test_parse_as_udpate_message() {
        let raw_dns_message = "xjUoAAABAAAAAQAAB2V4YW1wbGUDY29tAAAGAAECaDXADAD/AP8AAAAAAAA=";
        if let Ok(raw_update_message) = base64::decode(raw_dns_message) {
            let parse_result = DnsMessageParser::new(raw_update_message).parse_as_update_message();
            assert!(parse_result.is_ok());
            if let Ok(message) = parse_result {
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
            assert_eq!(r#""abc\"def" "gh\\i" "" "j""#, parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_tlsa_type() {
        let rdata = RData::TLSA(TLSA::new(
            CertUsage::Service,
            Selector::Spki,
            Matching::Sha256,
            vec![1, 2, 3, 4, 5, 6, 7, 8],
        ));
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!("1 1 1 0102030405060708", parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_sshfp_type() {
        let rdata = RData::SSHFP(SSHFP::new(
            Algorithm::ECDSA,
            FingerprintType::SHA1,
            vec![115, 115, 104, 102, 112],
        ));
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!("3 1 7373686670", parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_naptr_type() {
        let rdata1 = RData::NAPTR(NAPTR::new(
            8,
            16,
            b"aa11AA-".to_vec().into_boxed_slice(),
            b"services".to_vec().into_boxed_slice(),
            b"regexpr".to_vec().into_boxed_slice(),
            Name::from_str("naptr.example.com").unwrap(),
        ));
        let rdata_text1 = format_rdata(&rdata1);

        let rdata2 = RData::NAPTR(NAPTR::new(
            8,
            16,
            b"aa1\"\\1AA-".to_vec().into_boxed_slice(),
            b"\\services2\"".to_vec().into_boxed_slice(),
            b"re%ge\"xp.r\\".to_vec().into_boxed_slice(),
            Name::from_str("naptr.example.com").unwrap(),
        ));
        let rdata_text2 = format_rdata(&rdata2);

        assert!(rdata_text1.is_ok());
        assert!(rdata_text2.is_ok());

        if let Ok((parsed, raw_rdata)) = rdata_text1 {
            assert!(raw_rdata.is_none());
            assert_eq!(
                "8 16 \"aa11AA-\" \"services\" \"regexpr\" naptr.example.com",
                parsed.unwrap()
            );
        }

        if let Ok((parsed, raw_rdata)) = rdata_text2 {
            assert!(raw_rdata.is_none());
            assert_eq!(
                "8 16 \"aa1\\\"\\\\1AA-\" \"\\\\services2\\\"\" \"re%ge\\\"xp.r\\\\\" naptr.example.com",
                parsed.unwrap()
            );
        }
    }

    #[test]
    fn test_format_rdata_for_dnskey_type() {
        let rdata1 = RData::DNSSEC(DNSSECRData::DNSKEY(DNSKEY::new(
            true,
            true,
            false,
            DNSSEC_Algorithm::RSASHA256,
            vec![0, 1, 2, 3, 4, 5, 6, 7],
        )));
        let rdata_text1 = format_rdata(&rdata1);

        let rdata2 = RData::DNSSEC(DNSSECRData::DNSKEY(DNSKEY::new(
            true,
            false,
            false,
            DNSSEC_Algorithm::RSASHA256,
            vec![0, 1, 2, 3, 4, 5, 6, 7],
        )));
        let rdata_text2 = format_rdata(&rdata2);

        let rdata3 = RData::DNSSEC(DNSSECRData::DNSKEY(DNSKEY::new(
            true,
            true,
            true,
            DNSSEC_Algorithm::RSASHA256,
            vec![0, 1, 2, 3, 4, 5, 6, 7],
        )));
        let rdata_text3 = format_rdata(&rdata3);

        assert!(rdata_text1.is_ok());
        assert!(rdata_text2.is_ok());
        assert!(rdata_text3.is_ok());

        if let Ok((parsed, raw_rdata)) = rdata_text1 {
            assert!(raw_rdata.is_none());
            assert_eq!("257 3 8 AAECAwQFBgc=", parsed.unwrap());
        }
        if let Ok((parsed, raw_rdata)) = rdata_text2 {
            assert!(raw_rdata.is_none());
            assert_eq!("256 3 8 AAECAwQFBgc=", parsed.unwrap());
        }
        if let Ok((parsed, raw_rdata)) = rdata_text3 {
            assert!(raw_rdata.is_none());
            assert_eq!("0 3 8 AAECAwQFBgc=", parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_nsec_type() {
        let rdata = RData::DNSSEC(DNSSECRData::NSEC(NSEC::new(
            Name::from_str("www.example.com").unwrap(),
            vec![RecordType::A, RecordType::AAAA],
        )));
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!("www.example.com A AAAA", parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_nsec3_type() {
        let rdata = RData::DNSSEC(DNSSECRData::NSEC3(NSEC3::new(
            Nsec3HashAlgorithm::SHA1,
            true,
            2,
            vec![1, 2, 3, 4, 5],
            vec![6, 7, 8, 9, 0],
            vec![RecordType::A, RecordType::AAAA],
        )));
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!("1 1 2 0102030405 0O3GG280 A AAAA", parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_nsec3param_type() {
        let rdata = RData::DNSSEC(DNSSECRData::NSEC3PARAM(NSEC3PARAM::new(
            Nsec3HashAlgorithm::SHA1,
            true,
            2,
            vec![1, 2, 3, 4, 5],
        )));
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!("1 1 2 0102030405", parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_sig_type() {
        let rdata = RData::DNSSEC(DNSSECRData::SIG(SIG::new(
            RecordType::NULL,
            DNSSEC_Algorithm::RSASHA256,
            0,
            0,
            2,
            1,
            5,
            Name::from_str("www.example.com").unwrap(),
            vec![
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                23, 24, 25, 26, 27, 28, 29, 29, 31,
            ],
        )));
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!(
                "NULL 8 0 0 2 1 5 www.example.com AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHR8=",
                parsed.unwrap()
            );
        }
    }

    #[test]
    fn test_format_rdata_for_ds_type() {
        let rdata = RData::DNSSEC(DNSSECRData::DS(DS::new(
            0xF00F,
            DNSSEC_Algorithm::RSASHA256,
            DigestType::SHA256,
            vec![5, 6, 7, 8],
        )));
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!("61455 8 2 05060708", parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_hinfo_type() {
        if let Ok(raw_rdata) = base64::decode("BWludGVsBWxpbnV4") {
            let mut decoder = BinDecoder::new(&raw_rdata);
            let hinfo_rdata =
                null::read(&mut decoder, Restrict::new(raw_rdata.len() as u16)).unwrap();
            let rdata_text =
                DnsMessageParser::new(Vec::<u8>::new()).format_unknown_rdata(13, &hinfo_rdata);
            assert!(rdata_text.is_ok());
            assert_eq!("\"intel\" \"linux\"", rdata_text.unwrap().0.unwrap());
        } else {
            error!("Invalid base64 encoded rdata");
        }
    }

    #[test]
    fn test_format_rdata_for_minfo_type() {
        if let Ok(raw_message) = base64::decode("vuOFgAABAAEAAAABBm1pbmZvMQdleGFtcGxlA2NvbQAA\
        DgABwAwADgABAAAAyAANBGZyZWTAEwNqb2XAEwAAKRAAAAAAAAAcAAoAGM1lXY9c0es3AQAAAF8wHrTh8EfNB4O+ig=="){
            let raw_message_len = raw_message.len();
            let mut message_parser = DnsMessageParser::new(raw_message);
            if let Ok(raw_rdata) = base64::decode("BGZyZWTAEwNqb2XAEw==") {
                for i in 1..=2 {
                    let mut decoder = BinDecoder::new(&raw_rdata);
                    let minfo_rdata = null::read(&mut decoder, Restrict::new(raw_rdata.len() as u16)).unwrap();
                    let rdata_text = message_parser.format_unknown_rdata(14, &minfo_rdata);
                    assert!(rdata_text.is_ok());
                    assert_eq!("fred.example.com. joe.example.com.", rdata_text.unwrap().0.unwrap());
                    assert_eq!(raw_message_len + i * raw_rdata.len(), message_parser.raw_message_clone().unwrap().len());
                }
            } else {
                error!("Invalid base64 encoded raw rdata");
            }
        }else{
            error!("Invalid base64 encoded raw message");
        }
    }

    #[test]
    #[ignore]
    fn benchmark_parse_as_query_message() {
        let raw_dns_message = "szgAAAABAAAAAAAAAmg1B2V4YW1wbGUDY29tAAAGAAE=";
        if let Ok(raw_qeury_message) = base64::decode(raw_dns_message) {
            let start = Instant::now();
            let num = 10_000;
            for _ in 0..num {
                let parse_result =
                    DnsMessageParser::new(raw_qeury_message.clone()).parse_as_query_message();
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
