#[readonly::make]
#[derive(Debug)]
pub struct DnsQueryMessage {
    pub header: QueryHeader,
    pub question_section: Vec<QueryQuestion>,
    pub answer_section: Vec<DnsRecord>,
    pub authority_section: Vec<DnsRecord>,
    pub additional_section: Vec<DnsRecord>,
    pub opt_pserdo_section: Option<OptPseudoSection>,
}

impl DnsQueryMessage {
    pub fn new(
        header: QueryHeader,
        question_section: Vec<QueryQuestion>,
        answer_section: Vec<DnsRecord>,
        authority_section: Vec<DnsRecord>,
        additional_section: Vec<DnsRecord>,
        opt_pserdo_section: Option<OptPseudoSection>,
    ) -> Self {
        Self {
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
#[derive(Debug)]
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
#[derive(Debug)]
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
#[derive(Debug)]
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
#[derive(Debug)]
pub struct DnsRecord {
    pub name: String,
    pub class: String,
    pub record_type: Option<String>,
    pub record_type_id: u16,
    pub ttl: u32,
    pub rdata: String,
}

impl DnsRecord {
    pub fn new(
        name: String,
        class: String,
        record_type: Option<String>,
        record_type_id: u16,
        ttl: u32,
        rdata: String,
    ) -> Self {
        Self {
            name,
            class,
            record_type,
            record_type_id,
            ttl,
            rdata,
        }
    }
}

#[readonly::make]
#[derive(Debug)]
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
