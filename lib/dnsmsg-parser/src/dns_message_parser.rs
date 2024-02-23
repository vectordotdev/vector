use std::fmt::Write as _;
use std::str::Utf8Error;

use data_encoding::{BASE32HEX_NOPAD, BASE64, HEXUPPER};
use hickory_proto::{
    error::ProtoError,
    op::{message::Message as TrustDnsMessage, Edns, Query},
    rr::{
        dnssec::{rdata::DNSSECRData, Algorithm, SupportedAlgorithms},
        rdata::{
            caa::Value,
            opt::{EdnsCode, EdnsOption},
            A, AAAA, NULL, SVCB,
        },
        record_data::RData,
        resource::Record,
        Name, RecordType,
    },
    serialize::binary::{BinDecodable, BinDecoder},
};
use thiserror::Error;

use super::dns_message::{
    self, DnsQueryMessage, DnsRecord, DnsUpdateMessage, EdnsOptionEntry, OptPseudoSection,
    QueryHeader, QueryQuestion, UpdateHeader, ZoneInfo,
};

/// Error type for DNS message parsing
#[derive(Error, Debug)]
pub enum DnsMessageParserError {
    #[error("Encountered error : {}", cause)]
    SimpleError { cause: String },
    #[error("Encountered error from TrustDns: {}", source.to_string())]
    TrustDnsError { source: ProtoError },
    #[error("UTF8Error: {}", source)]
    Utf8ParsingError { source: Utf8Error },
}

/// Result alias for parsing
pub type DnsParserResult<T> = Result<T, DnsMessageParserError>;

/// A DNS message parser
#[derive(Debug)]
pub struct DnsMessageParser {
    raw_message: Vec<u8>,
    // This field is used to facilitate parsing of compressed domain names contained
    // in some record data. We could instantiate a new copy of the raw_message whenever
    // we need to parse rdata, but to avoid impact on performance, we'll instantiate
    // only one copy of raw_message upon the first call to parse an rdata that may
    // contain compressed domain name, and store it here as a member field; for
    // subsequent invocations of the same call, we simply reuse this copy.
    raw_message_for_rdata_parsing: Option<Vec<u8>>,
}

impl DnsMessageParser {
    pub fn new(raw_message: Vec<u8>) -> Self {
        DnsMessageParser {
            raw_message,
            raw_message_for_rdata_parsing: None,
        }
    }

    pub fn raw_message(&self) -> &[u8] {
        &self.raw_message
    }

    pub fn parse_as_query_message(&mut self) -> DnsParserResult<DnsQueryMessage> {
        let msg = TrustDnsMessage::from_vec(&self.raw_message)
            .map_err(|source| DnsMessageParserError::TrustDnsError { source })?;
        let header = parse_dns_query_message_header(&msg);
        let edns_section = parse_edns(&msg).transpose()?;
        let rcode_high = edns_section.as_ref().map_or(0, |edns| edns.extended_rcode);
        let response_code = (u16::from(rcode_high) << 4) | ((u16::from(header.rcode)) & 0x000F);

        Ok(DnsQueryMessage {
            response_code,
            response: parse_response_code(response_code),
            header,
            question_section: self.parse_dns_query_message_question_section(&msg),
            answer_section: self.parse_dns_message_section(msg.answers())?,
            authority_section: self.parse_dns_message_section(msg.name_servers())?,
            additional_section: self.parse_dns_message_section(msg.additionals())?,
            opt_pseudo_section: edns_section,
        })
    }

    pub fn parse_as_update_message(&mut self) -> DnsParserResult<DnsUpdateMessage> {
        let msg = TrustDnsMessage::from_vec(&self.raw_message)
            .map_err(|source| DnsMessageParserError::TrustDnsError { source })?;
        let header = parse_dns_update_message_header(&msg);
        let response_code = (u16::from(header.rcode)) & 0x000F;
        Ok(DnsUpdateMessage {
            response_code,
            response: parse_response_code(response_code),
            header,
            zone_to_update: self.parse_dns_update_message_zone_section(&msg)?,
            prerequisite_section: self.parse_dns_message_section(msg.answers())?,
            update_section: self.parse_dns_message_section(msg.name_servers())?,
            additional_section: self.parse_dns_message_section(msg.additionals())?,
        })
    }

    fn parse_dns_query_message_question_section(
        &self,
        dns_message: &TrustDnsMessage,
    ) -> Vec<QueryQuestion> {
        dns_message
            .queries()
            .iter()
            .map(|query| self.parse_dns_query_question(query))
            .collect()
    }

    fn parse_dns_query_question(&self, question: &Query) -> QueryQuestion {
        QueryQuestion {
            name: question.name().to_string(),
            class: question.query_class().to_string(),
            record_type: format_record_type(question.query_type()),
            record_type_id: u16::from(question.query_type()),
        }
    }

    fn parse_dns_update_message_zone_section(
        &self,
        dns_message: &TrustDnsMessage,
    ) -> DnsParserResult<ZoneInfo> {
        let zones = dns_message
            .queries()
            .iter()
            .map(|query| self.parse_dns_query_question(query).into())
            .collect::<Vec<ZoneInfo>>();

        zones
            .first()
            .cloned()
            .ok_or_else(|| DnsMessageParserError::SimpleError {
                cause: format!(
                    "Unexpected number of records in update section: {}",
                    zones.len()
                ),
            })
    }

    fn parse_dns_message_section(&mut self, records: &[Record]) -> DnsParserResult<Vec<DnsRecord>> {
        records
            .iter()
            .map(|record| self.parse_dns_record(record))
            .collect::<Result<Vec<_>, _>>()
    }

    fn parse_dns_record(&mut self, record: &Record) -> DnsParserResult<DnsRecord> {
        let record_data = match record.data() {
            Some(RData::Unknown { code, rdata }) => {
                self.format_unknown_rdata((*code).into(), rdata)
            }
            Some(rdata) => format_rdata(rdata),
            None => Ok((Some(String::from("")), None)), // NULL record
        }?;

        Ok(DnsRecord {
            name: record.name().to_string(),
            class: record.dns_class().to_string(),
            record_type: format_record_type(record.record_type()),
            record_type_id: u16::from(record.record_type()),
            ttl: record.ttl(),
            rdata: record_data.0,
            rdata_bytes: record_data.1,
        })
    }

    fn get_rdata_decoder_with_raw_message(&mut self, raw_rdata: &[u8]) -> BinDecoder<'_> {
        let (index, raw_message_for_rdata_parsing_data) =
            match self.raw_message_for_rdata_parsing.take() {
                Some(mut buf) => {
                    let index = buf.len();
                    buf.extend_from_slice(raw_rdata);
                    (index, buf)
                }
                None => {
                    let mut buf = Vec::<u8>::with_capacity(self.raw_message.len() * 2);
                    buf.extend(&self.raw_message);
                    buf.extend_from_slice(raw_rdata);
                    (self.raw_message.len(), buf)
                }
            };
        self.raw_message_for_rdata_parsing = Some(raw_message_for_rdata_parsing_data);

        BinDecoder::new(self.raw_message_for_rdata_parsing.as_ref().unwrap()).clone(index as u16)
    }

    fn parse_wks_rdata(
        &mut self,
        raw_rdata: &[u8],
    ) -> DnsParserResult<(Option<String>, Option<Vec<u8>>)> {
        let mut decoder = BinDecoder::new(raw_rdata);
        let address = parse_ipv4_address(&mut decoder)?;
        let protocol = parse_u8(&mut decoder)?;
        let port = {
            let mut port_string = String::new();
            let mut current_bit: u32 = 0;
            while !decoder.is_empty() {
                let mut current_byte = parse_u8(&mut decoder)?;
                if current_byte == 0 {
                    current_bit += 8;
                    continue;
                }
                for _i in 0..8 {
                    if current_byte & 0b1000_0000 == 0b1000_0000 {
                        write!(port_string, "{} ", current_bit)
                            .expect("can always write to String");
                    }
                    current_byte <<= 1;
                    current_bit += 1;
                }
            }
            port_string
        };
        Ok((
            Some(format!("{} {} {}", address, protocol, port.trim_end())),
            None,
        ))
    }

    fn parse_a6_rdata(
        &mut self,
        raw_rdata: &[u8],
    ) -> DnsParserResult<(Option<String>, Option<Vec<u8>>)> {
        let mut decoder = BinDecoder::new(raw_rdata);
        let prefix = parse_u8(&mut decoder)?;
        let ipv6_address = {
            let address_length = (128 - prefix) / 8;
            let mut address_vec = parse_vec(&mut decoder, address_length)?;
            if address_vec.len() < 16 {
                let pad_len = 16 - address_length;
                let mut padded_address_vec = vec![0; pad_len as usize];
                padded_address_vec.extend(&address_vec);
                address_vec = padded_address_vec;
            }
            let mut dec = BinDecoder::new(&address_vec);
            parse_ipv6_address(&mut dec)?
        };
        let domain_name = parse_domain_name(&mut decoder)?;
        Ok((
            Some(format!("{} {} {}", prefix, ipv6_address, domain_name)),
            None,
        ))
    }

    fn parse_loc_rdata(
        &mut self,
        raw_rdata: &[u8],
    ) -> DnsParserResult<(Option<String>, Option<Vec<u8>>)> {
        let mut decoder = BinDecoder::new(raw_rdata);
        let _max_latitude: u32 = 0x8000_0000 + 90 * 3_600_000;
        let _min_latitude: u32 = 0x8000_0000 - 90 * 3_600_000;
        let _max_longitude: u32 = 0x8000_0000 + 180 * 3_600_000;
        let _min_longitude: u32 = 0x8000_0000 - 180 * 3_600_000;
        let _version = parse_u8(&mut decoder)?;
        if _version != 0 {
            return Err(DnsMessageParserError::SimpleError {
                cause: String::from("LOC record version should be 0."),
            });
        }
        let size = parse_loc_rdata_size(parse_u8(&mut decoder)?)?;
        let horizontal_precision = parse_loc_rdata_size(parse_u8(&mut decoder)?)?;
        let vertical_precision = parse_loc_rdata_size(parse_u8(&mut decoder)?)?;

        let latitude = {
            let received_lat = parse_u32(&mut decoder)?;
            if received_lat < _min_latitude || received_lat > _max_latitude {
                return Err(DnsMessageParserError::SimpleError {
                    cause: String::from("LOC record latitude out of bounds"),
                });
            }
            let dir = if received_lat > 0x8000_0000 { "N" } else { "S" };
            parse_loc_rdata_coordinates(received_lat, dir)
        };

        let longitude = {
            let received_lon = parse_u32(&mut decoder)?;
            if received_lon < _min_longitude || received_lon > _max_longitude {
                return Err(DnsMessageParserError::SimpleError {
                    cause: String::from("LOC record longitude out of bounds"),
                });
            }
            let dir = if received_lon > 0x8000_0000 { "E" } else { "W" };
            parse_loc_rdata_coordinates(received_lon, dir)
        };
        let altitude = (parse_u32(&mut decoder)? as f64 - 10_000_000.0) / 100.0;

        Ok((
            Some(format!(
                "{} {} {:.2}m {}m {}m {}m",
                latitude, longitude, altitude, size, horizontal_precision, vertical_precision
            )),
            None,
        ))
    }

    fn parse_apl_rdata(
        &mut self,
        raw_rdata: &[u8],
    ) -> DnsParserResult<(Option<String>, Option<Vec<u8>>)> {
        let mut decoder = BinDecoder::new(raw_rdata);
        let mut apl_rdata = "".to_string();
        while !decoder.is_empty() {
            let address_family = parse_u16(&mut decoder)?;
            let prefix = parse_u8(&mut decoder)?;
            let mut afd_length = parse_u8(&mut decoder)?;
            let negation = if afd_length > 127 {
                afd_length -= 128;
                "!"
            } else {
                ""
            };
            let mut address_vec = parse_vec(&mut decoder, afd_length)?;
            let address = if address_family == 1 {
                if afd_length < 4 {
                    address_vec.resize(4, 0);
                }
                let mut dec = BinDecoder::new(&address_vec);
                parse_ipv4_address(&mut dec)?
            } else {
                if afd_length < 16 {
                    address_vec.resize(16, 0);
                }
                let mut dec = BinDecoder::new(&address_vec);
                parse_ipv6_address(&mut dec)?
            };
            write!(
                apl_rdata,
                "{}{}:{}/{}",
                negation, address_family, address, prefix
            )
            .expect("can always write to String");
            apl_rdata.push(' ');
        }
        Ok((Some(apl_rdata.trim_end().to_string()), None))
    }

    pub fn format_unknown_rdata(
        &mut self,
        code: u16,
        rdata: &NULL,
    ) -> DnsParserResult<(Option<String>, Option<Vec<u8>>)> {
        match code {
            dns_message::RTYPE_MB => {
                let mut decoder = self.get_rdata_decoder_with_raw_message(rdata.anything());
                let madname = parse_domain_name(&mut decoder)?;
                Ok((Some(madname.to_string()), None))
            }

            dns_message::RTYPE_MG => {
                let mut decoder = self.get_rdata_decoder_with_raw_message(rdata.anything());
                let mgname = parse_domain_name(&mut decoder)?;
                Ok((Some(mgname.to_string()), None))
            }

            dns_message::RTYPE_MR => {
                let mut decoder = self.get_rdata_decoder_with_raw_message(rdata.anything());
                let newname = parse_domain_name(&mut decoder)?;
                Ok((Some(newname.to_string()), None))
            }

            dns_message::RTYPE_WKS => self.parse_wks_rdata(rdata.anything()),

            dns_message::RTYPE_HINFO => {
                let mut decoder = BinDecoder::new(rdata.anything());
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

            dns_message::RTYPE_MINFO => {
                let mut decoder = self.get_rdata_decoder_with_raw_message(rdata.anything());
                let rmailbx = parse_domain_name(&mut decoder)?;
                let emailbx = parse_domain_name(&mut decoder)?;
                Ok((Some(format!("{} {}", rmailbx, emailbx)), None))
            }

            dns_message::RTYPE_RP => {
                let mut decoder = self.get_rdata_decoder_with_raw_message(rdata.anything());
                let mbox = parse_domain_name(&mut decoder)?;
                let txt = parse_domain_name(&mut decoder)?;
                Ok((Some(format!("{} {}", mbox, txt)), None))
            }

            dns_message::RTYPE_AFSDB => {
                let mut decoder = self.get_rdata_decoder_with_raw_message(rdata.anything());
                let subtype = parse_u16(&mut decoder)?;
                let hostname = parse_domain_name(&mut decoder)?;
                Ok((Some(format!("{} {}", subtype, hostname)), None))
            }

            dns_message::RTYPE_X25 => {
                let mut decoder = BinDecoder::new(rdata.anything());
                let psdn_address = parse_character_string(&mut decoder)?;
                Ok((
                    Some(format!(
                        "\"{}\"",
                        escape_string_for_text_representation(psdn_address)
                    )),
                    None,
                ))
            }

            dns_message::RTYPE_ISDN => {
                let mut decoder = BinDecoder::new(rdata.anything());
                let address = parse_character_string(&mut decoder)?;
                if decoder.is_empty() {
                    Ok((
                        Some(format!(
                            "\"{}\"",
                            escape_string_for_text_representation(address)
                        )),
                        None,
                    ))
                } else {
                    let sub_address = parse_character_string(&mut decoder)?;
                    Ok((
                        Some(format!(
                            "\"{}\" \"{}\"",
                            escape_string_for_text_representation(address),
                            escape_string_for_text_representation(sub_address)
                        )),
                        None,
                    ))
                }
            }

            dns_message::RTYPE_RT => {
                let mut decoder = self.get_rdata_decoder_with_raw_message(rdata.anything());
                let preference = parse_u16(&mut decoder)?;
                let intermediate_host = parse_domain_name(&mut decoder)?;
                Ok((Some(format!("{} {}", preference, intermediate_host)), None))
            }

            dns_message::RTYPE_NSAP => {
                let raw_rdata = rdata.anything();
                let mut decoder = BinDecoder::new(raw_rdata);
                let rdata_len = raw_rdata.len() as u16;
                let nsap_rdata = HEXUPPER.encode(&parse_vec_with_u16_len(&mut decoder, rdata_len)?);
                Ok((Some(format!("0x{}", nsap_rdata)), None))
            }

            dns_message::RTYPE_PX => {
                let mut decoder = self.get_rdata_decoder_with_raw_message(rdata.anything());
                let preference = parse_u16(&mut decoder)?;
                let map822 = parse_domain_name(&mut decoder)?;
                let mapx400 = parse_domain_name(&mut decoder)?;
                Ok((Some(format!("{} {} {}", preference, map822, mapx400)), None))
            }

            dns_message::RTYPE_LOC => self.parse_loc_rdata(rdata.anything()),

            dns_message::RTYPE_KX => {
                let mut decoder = self.get_rdata_decoder_with_raw_message(rdata.anything());
                let preference = parse_u16(&mut decoder)?;
                let exchanger = parse_domain_name(&mut decoder)?;
                Ok((Some(format!("{} {}", preference, exchanger)), None))
            }

            dns_message::RTYPE_CERT => {
                let raw_rdata = rdata.anything();
                let mut decoder = BinDecoder::new(raw_rdata);
                let cert_type = parse_u16(&mut decoder)?;
                let key_tag = parse_u16(&mut decoder)?;
                let algorithm = Algorithm::from_u8(parse_u8(&mut decoder)?).as_str();
                let crl_len = raw_rdata.len() as u16 - 5;
                let crl = BASE64.encode(&parse_vec_with_u16_len(&mut decoder, crl_len)?);
                Ok((
                    Some(format!("{} {} {} {}", cert_type, key_tag, algorithm, crl)),
                    None,
                ))
            }

            dns_message::RTYPE_A6 => self.parse_a6_rdata(rdata.anything()),

            dns_message::RTYPE_SINK => {
                let raw_rdata = rdata.anything();
                let mut decoder = BinDecoder::new(raw_rdata);
                let meaning = parse_u8(&mut decoder)?;
                let coding = parse_u8(&mut decoder)?;
                let subcoding = parse_u8(&mut decoder)?;
                let data_len = raw_rdata.len() as u16 - 3;
                let data = BASE64.encode(&parse_vec_with_u16_len(&mut decoder, data_len)?);

                Ok((
                    Some(format!("{} {} {} {}", meaning, coding, subcoding, data)),
                    None,
                ))
            }

            dns_message::RTYPE_APL => self.parse_apl_rdata(rdata.anything()),

            dns_message::RTYPE_DHCID => {
                let raw_rdata = rdata.anything();
                let mut decoder = BinDecoder::new(raw_rdata);
                let raw_data_len = raw_rdata.len() as u16;
                let digest = BASE64.encode(&parse_vec_with_u16_len(&mut decoder, raw_data_len)?);
                Ok((Some(digest), None))
            }

            dns_message::RTYPE_SPF => {
                let mut decoder = BinDecoder::new(rdata.anything());
                let mut text = String::new();
                while !decoder.is_empty() {
                    text.push('\"');
                    text.push_str(&parse_character_string(&mut decoder)?);
                    text.push_str("\" ");
                }
                Ok((Some(text.trim_end().to_string()), None))
            }

            _ => Ok((None, Some(rdata.anything().to_vec()))),
        }
    }
}

fn format_rdata(rdata: &RData) -> DnsParserResult<(Option<String>, Option<Vec<u8>>)> {
    match rdata {
        RData::A(ip) => Ok((Some(ip.to_string()), None)),
        RData::AAAA(ip) => Ok((Some(ip.to_string()), None)),
        RData::ANAME(name) => Ok((Some(name.to_string()), None)),
        RData::CNAME(name) => Ok((Some(name.to_string()), None)),
        RData::MX(mx) => {
            let srv_rdata = format!("{} {}", mx.preference(), mx.exchange(),);
            Ok((Some(srv_rdata), None))
        }
        RData::NULL(null) => Ok((Some(BASE64.encode(null.anything())), None)),
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
                soa.mname(),
                soa.rname(),
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
                srv.target()
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
        RData::CAA(caa) => {
            let caa_rdata = format!(
                "{} {} \"{}\"",
                caa.issuer_critical() as u8,
                caa.tag().as_str(),
                match caa.value() {
                    Value::Url(url) => {
                        url.as_str().to_string()
                    }
                    Value::Issuer(option_name, vec_keyvalue) => {
                        let mut final_issuer = String::new();
                        if let Some(name) = option_name {
                            final_issuer.push_str(&name.to_utf8());
                            for keyvalue in vec_keyvalue.iter() {
                                final_issuer.push_str("; ");
                                final_issuer.push_str(keyvalue.key());
                                final_issuer.push('=');
                                final_issuer.push_str(keyvalue.value());
                            }
                        }
                        final_issuer.trim_end().to_string()
                    }
                    Value::Unknown(unknown) => std::str::from_utf8(unknown)
                        .map_err(|source| DnsMessageParserError::Utf8ParsingError { source })?
                        .to_string(),
                }
            );
            Ok((Some(caa_rdata), None))
        }

        RData::TLSA(tlsa) => {
            let tlsa_rdata = format!(
                "{} {} {} {}",
                u8::from(tlsa.cert_usage()),
                u8::from(tlsa.selector()),
                u8::from(tlsa.matching()),
                HEXUPPER.encode(tlsa.cert_data())
            );
            Ok((Some(tlsa_rdata), None))
        }
        RData::SSHFP(sshfp) => {
            let sshfp_rdata = format!(
                "{} {} {}",
                Into::<u8>::into(sshfp.algorithm()),
                Into::<u8>::into(sshfp.fingerprint_type()),
                HEXUPPER.encode(sshfp.fingerprint())
            );
            Ok((Some(sshfp_rdata), None))
        }
        RData::NAPTR(naptr) => {
            let naptr_rdata = format!(
                r#"{} {} "{}" "{}" "{}" {}"#,
                naptr.order(),
                naptr.preference(),
                escape_string_for_text_representation(
                    std::str::from_utf8(naptr.flags())
                        .map_err(|source| DnsMessageParserError::Utf8ParsingError { source })?
                        .to_string()
                ),
                escape_string_for_text_representation(
                    std::str::from_utf8(naptr.services())
                        .map_err(|source| DnsMessageParserError::Utf8ParsingError { source })?
                        .to_string()
                ),
                escape_string_for_text_representation(
                    std::str::from_utf8(naptr.regexp())
                        .map_err(|source| DnsMessageParserError::Utf8ParsingError { source })?
                        .to_string()
                ),
                naptr.replacement().to_utf8()
            );
            Ok((Some(naptr_rdata), None))
        }
        RData::HTTPS(https) => {
            let https_data = format_svcb_record(&https.0);
            Ok((Some(https_data), None))
        }
        RData::SVCB(svcb) => {
            let svcb_data = format_svcb_record(svcb);
            Ok((Some(svcb_data), None))
        }
        RData::DNSSEC(dnssec) => match dnssec {
            // See https://tools.ietf.org/html/rfc4034 for details
            // on dnssec related rdata formats
            DNSSECRData::DS(ds) => {
                let ds_rdata = format!(
                    "{} {} {} {}",
                    ds.key_tag(),
                    u8::from(ds.algorithm()),
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
                            0b0000_0000_0000_0000
                        } else if dnskey.zone_key() && dnskey.secure_entry_point() {
                            0b0000_0001_0000_0001
                        } else {
                            0b0000_0001_0000_0000
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
                    nsec.next_domain_name(),
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
                    HEXUPPER.encode(nsec3.salt()),
                    BASE32HEX_NOPAD.encode(nsec3.next_hashed_owner_name()),
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
                    HEXUPPER.encode(nsec3param.salt()),
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
                    sig.signer_name(),
                    BASE64.encode(sig.sig())
                );
                Ok((Some(sig_rdata), None))
            }
            // RSIG is a derivation of SIG but choosing to keep this duplicate code in lieu of the alternative
            // which is to allocate to the heap with Box in order to deref.
            DNSSECRData::RRSIG(sig) => {
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
                    sig.signer_name(),
                    BASE64.encode(sig.sig())
                );
                Ok((Some(sig_rdata), None))
            }
            DNSSECRData::Unknown { code: _, rdata } => Ok((None, Some(rdata.anything().to_vec()))),
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

fn format_svcb_record(svcb: &SVCB) -> String {
    format!(
        "{} {} {}",
        svcb.svc_priority(),
        svcb.target_name(),
        svcb.svc_params()
            .iter()
            .map(|(key, value)| format!(r#"{}="{}""#, key, value.to_string().trim_end_matches(',')))
            .collect::<Vec<_>>()
            .join(" ")
    )
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
    QueryHeader {
        id: dns_message.header().id(),
        opcode: dns_message.header().op_code().into(),
        rcode: dns_message.header().response_code(),
        qr: dns_message.header().message_type() as u8,
        aa: dns_message.header().authoritative(),
        tc: dns_message.header().truncated(),
        rd: dns_message.header().recursion_desired(),
        ra: dns_message.header().recursion_available(),
        ad: dns_message.header().authentic_data(),
        cd: dns_message.header().checking_disabled(),
        question_count: dns_message.header().query_count(),
        answer_count: dns_message.header().answer_count(),
        authority_count: dns_message.header().name_server_count(),
        additional_count: dns_message.header().additional_count(),
    }
}

fn parse_dns_update_message_header(dns_message: &TrustDnsMessage) -> UpdateHeader {
    UpdateHeader {
        id: dns_message.header().id(),
        opcode: dns_message.header().op_code().into(),
        rcode: dns_message.header().response_code(),
        qr: dns_message.header().message_type() as u8,
        zone_count: dns_message.header().query_count(),
        prerequisite_count: dns_message.header().answer_count(),
        update_count: dns_message.header().name_server_count(),
        additional_count: dns_message.header().additional_count(),
    }
}

fn parse_edns(dns_message: &TrustDnsMessage) -> Option<DnsParserResult<OptPseudoSection>> {
    dns_message.extensions().as_ref().map(|edns| {
        parse_edns_options(edns).map(|options| OptPseudoSection {
            extended_rcode: edns.rcode_high(),
            version: edns.version(),
            dnssec_ok: edns.dnssec_ok(),
            udp_max_payload_size: edns.max_payload(),
            options,
        })
    })
}

fn parse_edns_options(edns: &Edns) -> DnsParserResult<Vec<EdnsOptionEntry>> {
    edns.options()
        .as_ref()
        .iter()
        .map(|(code, option)| match option {
            EdnsOption::DAU(algorithms)
            | EdnsOption::DHU(algorithms)
            | EdnsOption::N3U(algorithms) => {
                Ok(parse_edns_opt_dnssec_algorithms(*code, *algorithms))
            }
            EdnsOption::Unknown(_, opt_data) => Ok(parse_edns_opt(*code, opt_data)),
            option => Vec::<u8>::try_from(option)
                .map(|bytes| parse_edns_opt(*code, &bytes))
                .map_err(|source| DnsMessageParserError::TrustDnsError { source }),
        })
        .collect()
}

fn parse_edns_opt_dnssec_algorithms(
    opt_code: EdnsCode,
    algorithms: SupportedAlgorithms,
) -> EdnsOptionEntry {
    let algorithm_names: Vec<String> = algorithms.iter().map(|alg| alg.to_string()).collect();
    EdnsOptionEntry {
        opt_code: Into::<u16>::into(opt_code),
        opt_name: format!("{:?}", opt_code),
        opt_data: algorithm_names.join(" "),
    }
}

fn parse_edns_opt(opt_code: EdnsCode, opt_data: &[u8]) -> EdnsOptionEntry {
    EdnsOptionEntry {
        opt_code: Into::<u16>::into(opt_code),
        opt_name: format!("{:?}", opt_code),
        opt_data: BASE64.encode(opt_data),
    }
}

fn parse_loc_rdata_size(data: u8) -> DnsParserResult<f64> {
    let base = (data & 0xF0) >> 4;
    if base > 9 {
        return Err(DnsMessageParserError::SimpleError {
            cause: format!("The base shouldnt be greater than 9. Base: {}", base),
        });
    }

    let exponent = data & 0x0F;
    if exponent > 9 {
        return Err(DnsMessageParserError::SimpleError {
            cause: format!(
                "The exponent shouldnt be greater than 9. Exponent: {}",
                exponent
            ),
        });
    }

    let ten: u64 = 10;
    let ans = (base as f64) * ten.pow(exponent as u32) as f64;
    Ok(ans / 100.0) // convert cm to metre
}

fn parse_loc_rdata_coordinates(coordinates: u32, dir: &str) -> String {
    let degree = (coordinates as i64 - 0x8000_0000) as f64 / 3_600_000.00;
    let minute = degree.fract() * 60.0;
    let second = minute.fract() * 60.0;

    format!(
        "{} {} {:.3} {}",
        degree.trunc().abs(),
        minute.trunc().abs(),
        second.abs(),
        dir
    )
}

fn parse_character_string(decoder: &mut BinDecoder<'_>) -> DnsParserResult<String> {
    let raw_len = decoder
        .read_u8()
        .map_err(|source| DnsMessageParserError::TrustDnsError {
            source: ProtoError::from(source),
        })?;
    let len = raw_len.unverified() as usize;
    let raw_text =
        decoder
            .read_slice(len)
            .map_err(|source| DnsMessageParserError::TrustDnsError {
                source: ProtoError::from(source),
            })?;
    match raw_text.verify_unwrap(|r| r.len() == len) {
        Ok(verified_text) => Ok(String::from_utf8_lossy(verified_text).to_string()),
        Err(raw_data) => Err(DnsMessageParserError::SimpleError {
            cause: format!(
                "Unexpected data length: expected {}, got {}. Raw data {}",
                len,
                raw_data.len(),
                format_bytes_as_hex_string(raw_data)
            ),
        }),
    }
}

fn parse_u8(decoder: &mut BinDecoder<'_>) -> DnsParserResult<u8> {
    Ok(decoder
        .read_u8()
        .map_err(|source| DnsMessageParserError::TrustDnsError {
            source: ProtoError::from(source),
        })?
        .unverified())
}

fn parse_u16(decoder: &mut BinDecoder<'_>) -> DnsParserResult<u16> {
    Ok(decoder
        .read_u16()
        .map_err(|source| DnsMessageParserError::TrustDnsError {
            source: ProtoError::from(source),
        })?
        .unverified())
}

fn parse_u32(decoder: &mut BinDecoder<'_>) -> DnsParserResult<u32> {
    Ok(decoder
        .read_u32()
        .map_err(|source| DnsMessageParserError::TrustDnsError {
            source: ProtoError::from(source),
        })?
        .unverified())
}

fn parse_vec(decoder: &mut BinDecoder<'_>, buffer_len: u8) -> DnsParserResult<Vec<u8>> {
    let len = buffer_len as usize;
    Ok(decoder
        .read_vec(len)
        .map_err(|source| DnsMessageParserError::TrustDnsError {
            source: ProtoError::from(source),
        })?
        .unverified())
}

fn parse_vec_with_u16_len(
    decoder: &mut BinDecoder<'_>,
    buffer_len: u16,
) -> DnsParserResult<Vec<u8>> {
    let len = buffer_len as usize;
    Ok(decoder
        .read_vec(len)
        .map_err(|source| DnsMessageParserError::TrustDnsError {
            source: ProtoError::from(source),
        })?
        .unverified())
}

fn parse_ipv6_address(decoder: &mut BinDecoder<'_>) -> DnsParserResult<String> {
    Ok(<AAAA as BinDecodable>::read(decoder)
        .map_err(|source| DnsMessageParserError::TrustDnsError { source })?
        .to_string())
}

fn parse_ipv4_address(decoder: &mut BinDecoder<'_>) -> DnsParserResult<String> {
    Ok(<A as BinDecodable>::read(decoder)
        .map_err(|source| DnsMessageParserError::TrustDnsError { source })?
        .to_string())
}

fn parse_domain_name(decoder: &mut BinDecoder<'_>) -> DnsParserResult<Name> {
    Name::read(decoder).map_err(|source| DnsMessageParserError::TrustDnsError { source })
}

fn escape_string_for_text_representation(original_string: String) -> String {
    original_string.replace('\\', "\\\\").replace('\"', "\\\"")
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
    use std::{
        net::{Ipv4Addr, Ipv6Addr},
        str::FromStr,
    };

    use hickory_proto::rr::{
        dnssec::{
            rdata::{
                dnskey::DNSKEY, ds::DS, nsec::NSEC, nsec3::NSEC3, nsec3param::NSEC3PARAM, sig::SIG,
                DNSSECRData, RRSIG,
            },
            Algorithm as DNSSEC_Algorithm, DigestType, Nsec3HashAlgorithm,
        },
        domain::Name,
        rdata::{
            caa::KeyValue,
            sshfp::{Algorithm, FingerprintType},
            svcb,
            tlsa::{CertUsage, Matching, Selector},
            CAA, HTTPS, NAPTR, SSHFP, TLSA, TXT,
        },
    };

    use super::*;

    impl DnsMessageParser {
        pub fn raw_message_for_rdata_parsing(&self) -> Option<&Vec<u8>> {
            self.raw_message_for_rdata_parsing.as_ref()
        }
    }

    #[test]
    fn test_parse_as_query_message() {
        let raw_dns_message = "szgAAAABAAAAAAAAAmg1B2V4YW1wbGUDY29tAAAGAAE=";
        let raw_query_message = BASE64
            .decode(raw_dns_message.as_bytes())
            .expect("Invalid base64 encoded data.");
        let parse_result = DnsMessageParser::new(raw_query_message).parse_as_query_message();
        assert!(parse_result.is_ok());
        let message = parse_result.expect("Message is not parsed.");
        assert_eq!(message.header.qr, 0);
        assert_eq!(message.question_section.len(), 1);
        assert_eq!(
            message.question_section.first().unwrap().name,
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
    }

    #[test]
    fn test_parse_as_query_message_with_invalid_data() {
        let err = DnsMessageParser::new(vec![1, 2, 3])
            .parse_as_query_message()
            .expect_err("Expected TrustDnsError.");
        match err {
            DnsMessageParserError::TrustDnsError { source: e } => {
                assert_eq!(e.to_string(), "unexpected end of input reached")
            }
            DnsMessageParserError::SimpleError { cause: e } => {
                panic!("Expected TrustDnsError, got {}.", &e)
            }
            _ => panic!("{}.", err),
        }
    }

    #[test]
    fn test_parse_as_query_message_with_unsupported_rdata() {
        let raw_query_message_base64 = "eEaFgAABAAEAAAAABGRvYTEHZXhhbXBsZQNjb20AAQMAAcAMAQMAAQAADhAAIAAAAAAAAAAAAgIiImh0dHBzOi8vd3d3LmlzYy5vcmcv";
        let raw_query_message = BASE64
            .decode(raw_query_message_base64.as_bytes())
            .expect("Invalid base64 encoded data.");
        let dns_query_message = DnsMessageParser::new(raw_query_message)
            .parse_as_query_message()
            .expect("Invalid DNS query message.");
        assert_eq!(dns_query_message.answer_section[0].rdata, None);
        assert_ne!(dns_query_message.answer_section[0].rdata_bytes, None);
    }

    #[test]
    fn test_parse_response_with_https_rdata() {
        let raw_response_message_base64 = "Oe2BgAABAAEAAAABBGNkbnAHc2FuamFnaANjb20AAEEAAcAMAEEAAQAAASwAPQABAAABAAYCaDMCaDIABAAIrEDEHKxAxRwABgAgJgZHAADmAAAAAAAArEDEHCYGRwAA5gAAAAAAAKxAxRwAACkE0AAAAAAAHAAKABjWOVAgEGik/gEAAABlwiAuXkvEOviB1sk=";
        let raw_response_message = BASE64
            .decode(raw_response_message_base64.as_bytes())
            .expect("Invalid base64 encoded data.");
        let dns_response_message = DnsMessageParser::new(raw_response_message)
            .parse_as_query_message()
            .expect("Invalid DNS query message.");
        assert_eq!(
            dns_response_message.answer_section[0].rdata,
            Some(r#"1 . alpn="h3,h2" ipv4hint="172.64.196.28,172.64.197.28" ipv6hint="2606:4700:e6::ac40:c41c,2606:4700:e6::ac40:c51c""#.to_string())
        );
        assert_eq!(dns_response_message.answer_section[0].record_type_id, 65u16);
        assert_eq!(dns_response_message.answer_section[0].rdata_bytes, None);
    }

    #[test]
    fn test_format_bytes_as_hex_string() {
        assert_eq!(
            "01.02.03.AB.CD.EF",
            &format_bytes_as_hex_string(&[1, 2, 3, 0xab, 0xcd, 0xef])
        );
    }

    #[test]
    fn test_parse_unknown_record_type() {
        assert_eq!("A", parse_unknown_record_type(1).unwrap());
        assert_eq!("ANY", parse_unknown_record_type(255).unwrap());
        assert!(parse_unknown_record_type(22222).is_none());
    }

    #[test]
    fn test_parse_as_update_message_failure() {
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
        let raw_update_message = BASE64
            .decode(raw_dns_message.as_bytes())
            .expect("Invalid base64 encoded data.");
        assert!(DnsMessageParser::new(raw_update_message)
            .parse_as_update_message()
            .is_err());
    }

    #[test]
    fn test_parse_as_update_message() {
        let raw_dns_message = "xjUoAAABAAAAAQAAB2V4YW1wbGUDY29tAAAGAAECaDXADAD/AP8AAAAAAAA=";
        let raw_update_message = BASE64
            .decode(raw_dns_message.as_bytes())
            .expect("Invalid base64 encoded data.");
        let parse_result = DnsMessageParser::new(raw_update_message).parse_as_update_message();
        assert!(parse_result.is_ok());
        let message = parse_result.expect("Message is not parsed.");
        assert_eq!(message.header.qr, 0);
        assert_eq!(message.update_section.len(), 1);
        assert_eq!(message.update_section.first().unwrap().class, "ANY");
        assert_eq!(&message.zone_to_update.zone_type.clone().unwrap(), "SOA");
        assert_eq!(message.zone_to_update.name, "example.com.");
    }

    #[test]
    fn test_parse_loc_rdata_size() {
        let data: u8 = 51;
        let expected: f64 = 30.0;
        assert!((expected - parse_loc_rdata_size(data).unwrap()).abs() < f64::EPSILON);

        let data: u8 = 22;
        let expected: f64 = 10000.0;
        assert!((expected - parse_loc_rdata_size(data).unwrap()).abs() < f64::EPSILON);

        let data: u8 = 19;
        let expected: f64 = 10.0;
        assert!((expected - parse_loc_rdata_size(data).unwrap()).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_loc_rdata_coordinates() {
        let coordinates: u32 = 2299997648;
        let dir = "N";
        let expected = String::from("42 21 54.000 N");
        assert_eq!(expected, parse_loc_rdata_coordinates(coordinates, dir));

        let coordinates: u32 = 1891505648;
        let dir = "W";
        let expected = String::from("71 6 18.000 W");
        assert_eq!(expected, parse_loc_rdata_coordinates(coordinates, dir));
    }

    #[test]
    fn test_format_rdata_for_a_type() {
        let rdata = RData::A(Ipv4Addr::from_str("1.2.3.4").unwrap().into());
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!("1.2.3.4", parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_aaaa_type() {
        let rdata = RData::AAAA(Ipv6Addr::from_str("2001::1234").unwrap().into());
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!("2001::1234", parsed.unwrap());
        }
    }

    #[test]
    fn test_format_rdata_for_cname_type() {
        let rdata = RData::CNAME(hickory_proto::rr::rdata::CNAME(
            Name::from_str("www.example.com.").unwrap(),
        ));
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
    fn test_format_rdata_for_caa_type() {
        let rdata1 = RData::CAA(CAA::new_issue(
            true,
            Some(Name::parse("example.com", None).unwrap()),
            vec![],
        ));
        let rdata2 = RData::CAA(CAA::new_issue(
            true,
            Some(Name::parse("example.com", None).unwrap()),
            vec![KeyValue::new("key", "value")],
        ));
        let rdata_text1 = format_rdata(&rdata1);
        let rdata_text2 = format_rdata(&rdata2);

        assert!(rdata_text1.is_ok());
        assert!(rdata_text2.is_ok());

        if let Ok((parsed, raw_rdata)) = rdata_text1 {
            assert!(raw_rdata.is_none());
            assert_eq!("1 issue \"example.com\"", parsed.unwrap());
        }

        if let Ok((parsed, raw_rdata)) = rdata_text2 {
            assert!(raw_rdata.is_none());
            assert_eq!("1 issue \"example.com; key=value\"", parsed.unwrap());
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

    // rsig is a derivation of the SIG record data, but the upstream crate does not handle that with an trait
    // so there isn't really a great way to reduce code duplication here.
    #[test]
    fn test_format_rdata_for_rsig_type() {
        let rdata = RData::DNSSEC(DNSSECRData::RRSIG(RRSIG::new(
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
    fn test_format_rdata_for_svcb_type() {
        let rdata = RData::SVCB(svcb::SVCB::new(
            1,
            Name::root(),
            vec![
                (
                    svcb::SvcParamKey::Alpn,
                    svcb::SvcParamValue::Alpn(svcb::Alpn(vec!["h3".to_string(), "h2".to_string()])),
                ),
                (
                    svcb::SvcParamKey::Ipv4Hint,
                    svcb::SvcParamValue::Ipv4Hint(svcb::IpHint(vec![
                        A(Ipv4Addr::new(104, 18, 36, 155)),
                        A(Ipv4Addr::new(172, 64, 151, 101)),
                    ])),
                ),
            ],
        ));
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!(
                r#"1 . alpn="h3,h2" ipv4hint="104.18.36.155,172.64.151.101""#,
                parsed.unwrap()
            );
        }
    }

    #[test]
    fn test_format_rdata_for_https_type() {
        let rdata = RData::HTTPS(HTTPS(svcb::SVCB::new(
            1,
            Name::root(),
            vec![
                (
                    svcb::SvcParamKey::Alpn,
                    svcb::SvcParamValue::Alpn(svcb::Alpn(vec!["h3".to_string(), "h2".to_string()])),
                ),
                (
                    svcb::SvcParamKey::Ipv4Hint,
                    svcb::SvcParamValue::Ipv4Hint(svcb::IpHint(vec![
                        A(Ipv4Addr::new(104, 18, 36, 155)),
                        A(Ipv4Addr::new(172, 64, 151, 101)),
                    ])),
                ),
            ],
        )));
        let rdata_text = format_rdata(&rdata);
        assert!(rdata_text.is_ok());
        if let Ok((parsed, raw_rdata)) = rdata_text {
            assert!(raw_rdata.is_none());
            assert_eq!(
                r#"1 . alpn="h3,h2" ipv4hint="104.18.36.155,172.64.151.101""#,
                parsed.unwrap()
            );
        }
    }

    #[test]
    fn test_format_rdata_for_hinfo_type() {
        test_format_rdata("BWludGVsBWxpbnV4", 13, "\"intel\" \"linux\"");
    }

    #[test]
    fn test_format_rdata_for_minfo_type() {
        test_format_rdata_with_compressed_domain_names(
            "5ZWBgAABAAEAAAABBm1pbmZvbwhleGFtcGxlMQNjb20AAA4AAcAMAA4AAQAADGsADQRmcmVkwBMDam9lwBMAACkQAAAAAAAAHAAKABgZ5zwJEK3VJQEAAABfSBqpS2bKf9CNBXg=",
            "BGZyZWTAEwNqb2XAEw==",
            14,
            "fred.example1.com. joe.example1.com."
        );
    }

    #[test]
    fn test_format_rdata_for_mb_type() {
        test_format_rdata_with_compressed_domain_names(
            "t8eBgAABAAEAAAABAm1iCGV4YW1wbGUxA2NvbQAABwABwAwABwABAAAA5AAJBmFhYmJjY8APAA\
            ApEAAAAAAAABwACgAYedbJkVVpMhsBAAAAX0U+y6UJQtCd0MuPBmFhYmJjY8AP",
            "BmFhYmJjY8AP",
            7,
            "aabbcc.example1.com.",
        );
    }

    #[test]
    fn test_format_rdata_for_mg_type() {
        test_format_rdata_with_compressed_domain_names(
            "o8ABIAABAAAAAAABAm1nCGV4YW1wbGUxA2NvbQAACAABAAApEAAAAAAAAAwACgAICQ3LVdp9euQ=",
            "wAw=",
            8,
            "mg.example1.com.",
        );
    }

    #[test]
    fn test_format_rdata_for_mr_type() {
        test_format_rdata_with_compressed_domain_names(
            "VWQBIAABAAAAAAABAm1yCGV4YW1wbGUxA2NvbQAACQABAAApEAAAAAAAAAwACgAIaPayFPJ4rmY=",
            "wAw=",
            9,
            "mr.example1.com.",
        );
    }

    #[test]
    fn test_format_rdata_for_wks_type() {
        test_format_rdata("gAgBDgYAAAFA", 11, "128.8.1.14 6 23 25");

        test_format_rdata("gAgBDgYAAAE=", 11, "128.8.1.14 6 23");
    }

    #[test]
    fn test_format_rdata_for_rp_type() {
        test_format_rdata_with_compressed_domain_names(
            "Xc0BIAABAAAAAAABAnJwCGV4YW1wbGUxA2NvbQAAEQABAAApEAAAAAAAAAwACgAIMoUjsVrqjwo=",
            "BWxvdWllB3RyYW50b3IDdW1kA2VkdQAETEFNMQZwZW9wbGUDdW1kA2VkdQA=",
            17,
            "louie.trantor.umd.edu. LAM1.people.umd.edu.",
        );
    }

    #[test]
    fn test_format_rdata_for_afsdb_type() {
        test_format_rdata_with_compressed_domain_names(
            "uaMBIAABAAAAAAABBWFmc2RiCGV4YW1wbGUxA2NvbQAAEgABAAApEAAAAAAAAAwACgAINy\
            n/qwKTyVc=",
            "AAEHYmlnYmlyZAd0b2FzdGVyA2NvbQA=",
            18,
            "1 bigbird.toaster.com.",
        );
    }

    #[test]
    fn test_format_rdata_for_x25_type() {
        test_format_rdata("DDMxMTA2MTcwMDk1Ng==", 19, "\"311061700956\"");
    }

    #[test]
    fn test_format_rdata_for_isdn_type() {
        test_format_rdata("DzE1MDg2MjAyODAwMzIxNw==", 20, "\"150862028003217\"");
    }

    #[test]
    fn test_format_rdata_for_rt_type() {
        test_format_rdata_with_compressed_domain_names(
            "K1cBEAABAAAAAAABAnJ0CGV4YW1wbGUxA2NvbQAAFQABAAApAgAAAIAAABwACgAY4Rzxu\
            TfOxRwNw0bSX0VXy7WIF30GJ7DD",
            "AAoCYWEHZXhhbXBsZQNjb20A",
            21,
            "10 aa.example.com.",
        );
    }

    #[test]
    fn test_format_rdata_for_nsap_type() {
        test_format_rdata(
            "RwAFgABaAAAAAAHhM////wABYQA=",
            22,
            "0x47000580005A0000000001E133FFFFFF00016100",
        );
    }

    #[test]
    fn test_format_rdata_for_px_type() {
        test_format_rdata_with_compressed_domain_names(
            "QF+BgAABAAEAAAABAnB4CGV4YW1wbGUxA2NvbQAAGgABwAwAGgABAAAOEAAlAAoEbmV0\
            MgJpdAAJUFJNRC1uZXQyCUFETUQtcDQwMARDLWl0AAAAKRAAAAAAAAAcAAoAGDnSHBrTcxU1AQAAAF9FWK\
            fIBBM9awy20w==",
            "AAoEbmV0MgJpdAAJUFJNRC1uZXQyCUFETUQtcDQwMARDLWl0AA==",
            26,
            "10 net2.it. PRMD-net2.ADMD-p400.C-it.",
        );
    }

    #[test]
    fn test_format_rdata_for_loc_type() {
        test_format_rdata(
            "ADMWE4kXLdBwvhXwAJiNIA==",
            29,
            "42 21 54.000 N 71 6 18.000 W -24.00m 30m 10000m 10m",
        );
    }

    #[test]
    fn test_format_rdata_for_kx_type() {
        test_format_rdata_with_compressed_domain_names(
            "E4yBgAABAAEAAAABAmt4CGV4YW1wbGUxA2NvbQAAJAABwAwAJAABAAAOEAASAAoCYWEHZ\
            XhhbXBsZQNjb20AAAApEAAAAAAAABwACgAYohY6RsSf9dsBAAAAX0VY5DfEoTM1iq9G",
            "AAoCYWEHZXhhbXBsZQNjb20A",
            36,
            "10 aa.example.com.",
        );
    }

    #[test]
    fn test_format_rdata_for_cert_type() {
        test_format_rdata(
            "//7//wUzEVxvL2T/K950x9CArOEfl6vQy7+8gvPjkiSyRx4UaCJYKf8bEeFq\
            LpUC4cCg1TPhihTW1V9IJKpBifr//XVTo2V3zSMR4LxpOs74oqYJpg==",
            37,
            "65534 65535 RSASHA1 MxFcby9k/yvedMfQgKzhH5er0Mu/vILz4\
            5IkskceFGgiWCn/GxHhai6VAuHAoNUz4YoU1tVfSCSqQYn6//11U6Nld80jEeC8aTrO+KKmCaY=",
        );
    }

    #[test]
    fn test_format_rdata_for_a6_type() {
        test_format_rdata(
            "QBI0VniavN7wCFNVQk5FVC0xA0lQNghleGFtcGxlMQNjb20A",
            38,
            "64 ::1234:5678:9abc:def0 SUBNET-1.IP6.example1.com.",
        );
    }

    #[test]
    fn test_format_rdata_for_sink_type() {
        test_format_rdata("AQIDdddd", 40, "1 2 3 dddd");
    }

    #[test]
    fn test_format_rdata_for_apl_type() {
        test_format_rdata(
            "AAEVA8CoIAABHIPAqCY=",
            42,
            "1:192.168.32.0/21 !1:192.168.38.0/28",
        );

        test_format_rdata("AAEEAeAAAggB/w==", 42, "1:224.0.0.0/4 2:ff00::/8");

        test_format_rdata(
            "AAEVA8CoIAABHATAqCYsAAEdA8AAJgABHYPAACYAAR2EwAAmCA==",
            42,
            "1:192.168.32.0/21 1:192.168.38.44/28 \
            1:192.0.38.0/29 !1:192.0.38.0/29 !1:192.0.38.8/29",
        );

        test_format_rdata(
            "AAEVA8CoIAABHATAqCYsAAEdA8AAJg==",
            42,
            "1:192.168.32.0/21 1:192.168.38.44/28 1:192.0.38.0/29",
        );
    }

    #[test]
    fn test_format_rdata_for_dhcid_type() {
        test_format_rdata(
            "AAIBY2/AuCccgoJbsaxcQc9TUapptP69lOjxfNuVAA2kjEA=",
            49,
            "AAIBY2/AuCccgoJbsaxcQc9TUapptP69lOjxfNuVAA2kjEA=",
        );
    }

    #[test]
    fn test_format_rdata_for_spf_type() {
        test_format_rdata(
            "BnY9c3BmMQMrbXgVYTpjb2xvLmV4YW1wbGUuY29tLzI4BC1hbGw=",
            99,
            "\"v=spf1\" \"+mx\" \"a:colo.example.com/28\" \"-all\"",
        );
    }

    fn test_format_rdata(raw_data: &str, code: u16, expected_output: &str) {
        let raw_rdata = BASE64
            .decode(raw_data.as_bytes())
            .expect("Invalid base64 encoded rdata.");
        let record_rdata = NULL::with(raw_rdata);
        let rdata_text =
            DnsMessageParser::new(Vec::<u8>::new()).format_unknown_rdata(code, &record_rdata);
        assert!(rdata_text.is_ok());
        assert_eq!(expected_output, rdata_text.unwrap().0.unwrap());
    }

    fn test_format_rdata_with_compressed_domain_names(
        raw_message: &str,
        raw_data_encoded: &str,
        code: u16,
        expected_output: &str,
    ) {
        let raw_message = BASE64
            .decode(raw_message.as_bytes())
            .expect("Invalid base64 encoded raw message.");
        let raw_message_len = raw_message.len();
        let mut message_parser = DnsMessageParser::new(raw_message);
        let raw_rdata = BASE64
            .decode(raw_data_encoded.as_bytes())
            .expect("Invalid base64 encoded raw rdata.");
        for i in 1..=2 {
            let record_rdata = NULL::with(raw_rdata.clone());
            let rdata_text = message_parser.format_unknown_rdata(code, &record_rdata);
            assert!(rdata_text.is_ok());
            assert_eq!(expected_output, rdata_text.unwrap().0.unwrap());
            assert_eq!(
                raw_message_len + i * raw_rdata.len(),
                message_parser
                    .raw_message_for_rdata_parsing()
                    .unwrap()
                    .len()
            );
        }
    }
}
