#[readonly::make]
#[derive(Clone, Debug)]
pub struct DnsQueryMessage {
    pub response_code: u16,
    pub response: Option<&'static str>,
    pub header: QueryHeader,
    pub question_section: Vec<QueryQuestion>,
    pub answer_section: Vec<DnsRecord>,
    pub authority_section: Vec<DnsRecord>,
    pub additional_section: Vec<DnsRecord>,
    pub opt_pserdo_section: Option<OptPseudoSection>,
}

impl DnsQueryMessage {
    pub fn new(
        response_code: u16,
        response: Option<&'static str>,
        header: QueryHeader,
        question_section: Vec<QueryQuestion>,
        answer_section: Vec<DnsRecord>,
        authority_section: Vec<DnsRecord>,
        additional_section: Vec<DnsRecord>,
        opt_pserdo_section: Option<OptPseudoSection>,
    ) -> Self {
        Self {
            response_code,
            response,
            header,
            question_section,
            answer_section,
            authority_section,
            additional_section,
            opt_pserdo_section,
        }
    }
}

#[readonly::make]
#[derive(Clone, Debug)]
pub struct QueryHeader {
    pub id: u16,
    pub opcode: u8,
    pub rcode: u8,
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

impl QueryHeader {
    pub fn new(
        id: u16,
        opcode: u8,
        rcode: u8,
        qr: u8,
        aa: bool,
        tc: bool,
        rd: bool,
        ra: bool,
        ad: bool,
        cd: bool,
        question_count: u16,
        answer_count: u16,
        authority_count: u16,
        additional_count: u16,
    ) -> Self {
        Self {
            id,
            opcode,
            rcode,
            qr,
            aa,
            tc,
            rd,
            ra,
            ad,
            cd,
            question_count,
            answer_count,
            authority_count,
            additional_count,
        }
    }
}

#[readonly::make]
#[derive(Clone, Debug)]
pub struct DnsUpdateMessage {
    pub response_code: u16,
    pub response: Option<&'static str>,
    pub header: UpdateHeader,
    pub zone_to_update: ZoneInfo,
    pub prerequisite_section: Vec<DnsRecord>,
    pub update_section: Vec<DnsRecord>,
    pub additional_section: Vec<DnsRecord>,
    pub opt_pserdo_section: Option<OptPseudoSection>,
}

impl DnsUpdateMessage {
    pub fn new(
        response_code: u16,
        response: Option<&'static str>,
        header: UpdateHeader,
        zone_to_update: ZoneInfo,
        prerequisite_section: Vec<DnsRecord>,
        update_section: Vec<DnsRecord>,
        additional_section: Vec<DnsRecord>,
        opt_pserdo_section: Option<OptPseudoSection>,
    ) -> Self {
        Self {
            response_code,
            response,
            header,
            zone_to_update,
            prerequisite_section,
            update_section,
            additional_section,
            opt_pserdo_section,
        }
    }
}

#[readonly::make]
#[derive(Clone, Debug)]
pub struct UpdateHeader {
    pub id: u16,
    pub opcode: u8,
    pub rcode: u8,
    pub qr: u8,
    pub zone_count: u16,
    pub prerequisite_count: u16,
    pub update_count: u16,
    pub additional_count: u16,
}

impl UpdateHeader {
    pub fn new(
        id: u16,
        opcode: u8,
        rcode: u8,
        qr: u8,
        zone_count: u16,
        prerequisite_count: u16,
        update_count: u16,
        additional_count: u16,
    ) -> Self {
        Self {
            id,
            opcode,
            rcode,
            qr,
            zone_count,
            prerequisite_count,
            update_count,
            additional_count,
        }
    }
}

#[readonly::make]
#[derive(Clone, Debug)]
pub struct OptPseudoSection {
    pub extended_rcode: u8,
    pub version: u8,
    pub dnssec_ok: bool,
    pub udp_max_payload_size: u16,
    pub options: Vec<EdnsOptionEntry>,
}

impl OptPseudoSection {
    pub fn new(
        extended_rcode: u8,
        version: u8,
        dnssec_ok: bool,
        udp_max_payload_size: u16,
        options: Vec<EdnsOptionEntry>,
    ) -> Self {
        Self {
            extended_rcode,
            version,
            dnssec_ok,
            udp_max_payload_size,
            options,
        }
    }
}

#[readonly::make]
#[derive(Clone, Debug)]
pub struct QueryQuestion {
    pub name: String,
    pub class: String,
    pub record_type: Option<String>,
    pub record_type_id: u16,
}

impl QueryQuestion {
    pub fn new(
        name: String,
        class: String,
        record_type: Option<String>,
        record_type_id: u16,
    ) -> Self {
        Self {
            name,
            class,
            record_type,
            record_type_id,
        }
    }
}

#[readonly::make]
#[derive(Clone, Debug)]
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

#[readonly::make]
#[derive(Clone, Debug)]
pub struct DnsRecord {
    pub name: String,
    pub class: String,
    pub record_type: Option<String>,
    pub record_type_id: u16,
    pub ttl: u32,
    pub rdata: Option<String>,
    pub rdata_bytes: Option<Vec<u8>>,
}

impl DnsRecord {
    pub fn new(
        name: String,
        class: String,
        record_type: Option<String>,
        record_type_id: u16,
        ttl: u32,
        rdata: Option<String>,
        rdata_bytes: Option<Vec<u8>>,
    ) -> Self {
        Self {
            name,
            class,
            record_type,
            record_type_id,
            ttl,
            rdata,
            rdata_bytes,
        }
    }
}

#[readonly::make]
#[derive(Clone, Debug)]
pub struct EdnsOptionEntry {
    pub opt_code: u16,
    pub opt_name: String,
    pub opt_data: String,
}

impl EdnsOptionEntry {
    pub fn new(opt_code: u16, opt_name: String, opt_data: String) -> Self {
        Self {
            opt_code,
            opt_name,
            opt_data,
        }
    }
}
