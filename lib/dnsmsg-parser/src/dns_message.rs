use hickory_proto::op::ResponseCode;

use crate::ede::EDE;

pub(super) const RTYPE_MB: u16 = 7;
pub(super) const RTYPE_MG: u16 = 8;
pub(super) const RTYPE_MR: u16 = 9;
pub(super) const RTYPE_WKS: u16 = 11;
pub(super) const RTYPE_MINFO: u16 = 14;
pub(super) const RTYPE_RP: u16 = 17;
pub(super) const RTYPE_AFSDB: u16 = 18;
pub(super) const RTYPE_X25: u16 = 19;
pub(super) const RTYPE_ISDN: u16 = 20;
pub(super) const RTYPE_RT: u16 = 21;
pub(super) const RTYPE_NSAP: u16 = 22;
pub(super) const RTYPE_PX: u16 = 26;
pub(super) const RTYPE_LOC: u16 = 29;
pub(super) const RTYPE_KX: u16 = 36;
pub(super) const RTYPE_CERT: u16 = 37;
pub(super) const RTYPE_A6: u16 = 38;
pub(super) const RTYPE_SINK: u16 = 40;
pub(super) const RTYPE_APL: u16 = 42;
pub(super) const RTYPE_DHCID: u16 = 49;
pub(super) const RTYPE_SPF: u16 = 99;

#[derive(Clone, Debug, Default)]
pub struct DnsQueryMessage {
    pub response_code: u16,
    pub response: Option<&'static str>,
    pub header: QueryHeader,
    pub question_section: Vec<QueryQuestion>,
    pub answer_section: Vec<DnsRecord>,
    pub authority_section: Vec<DnsRecord>,
    pub additional_section: Vec<DnsRecord>,
    pub opt_pseudo_section: Option<OptPseudoSection>,
}

#[derive(Clone, Debug, Default)]
pub struct QueryHeader {
    pub id: u16,
    pub opcode: u8,
    pub rcode: ResponseCode,
    pub qr: u8,
    pub aa: bool,
    pub tc: bool,
    pub rd: bool,
    pub ra: bool,
    pub ad: bool,
    pub cd: bool,
    pub question_count: u16,
    pub answer_count: u16,
    pub authority_count: u16,
    pub additional_count: u16,
}

#[derive(Clone, Debug, Default)]
pub struct DnsUpdateMessage {
    pub response_code: u16,
    pub response: Option<&'static str>,
    pub header: UpdateHeader,
    pub zone_to_update: ZoneInfo,
    pub prerequisite_section: Vec<DnsRecord>,
    pub update_section: Vec<DnsRecord>,
    pub additional_section: Vec<DnsRecord>,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateHeader {
    pub id: u16,
    pub opcode: u8,
    pub rcode: ResponseCode,
    pub qr: u8,
    pub zone_count: u16,
    pub prerequisite_count: u16,
    pub update_count: u16,
    pub additional_count: u16,
}

#[derive(Clone, Debug, Default)]
pub struct OptPseudoSection {
    pub extended_rcode: u8,
    pub version: u8,
    pub dnssec_ok: bool,
    pub udp_max_payload_size: u16,
    pub ede: Vec<EDE>,
    pub options: Vec<EdnsOptionEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct QueryQuestion {
    pub name: String,
    pub class: String,
    pub record_type: Option<String>,
    pub record_type_id: u16,
}

#[derive(Clone, Debug, Default)]
pub struct ZoneInfo {
    pub name: String,
    pub class: String,
    pub zone_type: Option<String>,
    pub zone_type_id: u16,
}

impl From<QueryQuestion> for ZoneInfo {
    fn from(query: QueryQuestion) -> Self {
        Self {
            name: query.name,
            class: query.class,
            zone_type: query.record_type,
            zone_type_id: query.record_type_id,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DnsRecord {
    pub name: String,
    pub class: String,
    pub record_type: Option<String>,
    pub record_type_id: u16,
    pub ttl: u32,
    pub rdata: Option<String>,
    pub rdata_bytes: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default)]
pub struct EdnsOptionEntry {
    pub opt_code: u16,
    pub opt_name: String,
    pub opt_data: String,
}
