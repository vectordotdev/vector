#[derive(Default)]
pub struct DnstapEventSchema {
    dnstap_data_schema: DnstapDataSchema,
    dns_query_message_schema: DnsQueryMessageSchema,
    dns_query_header_schema: DnsQueryHeaderSchema,
    dns_message_opt_pseudo_section_schema: DnsMessageOptPseudoSectionSchema,
    dns_message_option_schema: DnsMessageOptionSchema,
    dns_record_schema: DnsRecordSchema,
}

impl DnstapEventSchema {
    pub fn new() -> Self {
        Self {
            dnstap_data_schema: DnstapDataSchema::default(),
            dns_query_message_schema: DnsQueryMessageSchema::default(),
            dns_query_header_schema: DnsQueryHeaderSchema::default(),
            dns_message_opt_pseudo_section_schema: DnsMessageOptPseudoSectionSchema::default(),
            dns_message_option_schema: DnsMessageOptionSchema::default(),
            dns_record_schema: DnsRecordSchema::default(),
        }
    }

    pub fn dnstap_data_schema(self: &Self) -> &DnstapDataSchema {
        &self.dnstap_data_schema
    }

    pub fn dns_query_message_schema(self: &Self) -> &DnsQueryMessageSchema {
        &self.dns_query_message_schema
    }

    pub fn dns_query_header_schema(self: &Self) -> &DnsQueryHeaderSchema {
        &self.dns_query_header_schema
    }

    pub fn dns_message_opt_pseudo_section_schema(self: &Self) -> &DnsMessageOptPseudoSectionSchema {
        &self.dns_message_opt_pseudo_section_schema
    }

    pub fn dns_message_option_schema(self: &Self) -> &DnsMessageOptionSchema {
        &self.dns_message_option_schema
    }

    pub fn dns_record_schema(self: &Self) -> &DnsRecordSchema {
        &self.dns_record_schema
    }
}

pub struct DnstapDataSchema {
    server_identity: String,
    server_version: String,
    extra: String,
    data_type: String,
    message: DnstapMessageSchema,
    error: String,
    raw_data: String,
}

impl Default for DnstapDataSchema {
    fn default() -> Self {
        Self {
            server_identity: String::from("server_identity"),
            server_version: String::from("server_version"),
            extra: String::from("extra"),
            data_type: String::from("type"),
            message: DnstapMessageSchema::default(),
            error: String::from("error"),
            raw_data: String::from("data.raw_data"),
        }
    }
}

impl DnstapDataSchema {
    pub fn server_identity(self: &Self) -> &str {
        &self.server_identity
    }
    pub fn server_version(self: &Self) -> &str {
        &self.server_version
    }
    pub fn extra(self: &Self) -> &str {
        &self.extra
    }
    pub fn data_type(self: &Self) -> &str {
        &self.data_type
    }
    pub fn message(self: &Self) -> &DnstapMessageSchema {
        &self.message
    }
    pub fn error(self: &Self) -> &str {
        &self.error
    }
    pub fn raw_data(self: &Self) -> &str {
        &self.raw_data
    }
}

pub struct DnstapMessageSchema {
    socket_family: String,
    socket_protocol: String,
    query_address: String,
    query_port: String,
    response_address: String,
    response_port: String,
    query_zone: String,
    query_time_sec: String,
    query_time_nsec: String,
    response_time_sec: String,
    response_time_nsec: String,
    dnstap_message_type: String,
    query_message: String,
    response_message: String,
}

impl DnstapMessageSchema {
    pub fn socket_family(self: &Self) -> &str {
        &self.socket_family
    }
    pub fn socket_protocol(self: &Self) -> &str {
        &self.socket_protocol
    }
    pub fn query_address(self: &Self) -> &str {
        &self.query_address
    }
    pub fn query_port(self: &Self) -> &str {
        &self.query_port
    }
    pub fn response_address(self: &Self) -> &str {
        &self.response_address
    }
    pub fn response_port(self: &Self) -> &str {
        &self.response_port
    }
    pub fn query_zone(self: &Self) -> &str {
        &self.query_zone
    }
    pub fn query_time_sec(self: &Self) -> &str {
        &self.query_time_sec
    }
    pub fn query_time_nsec(self: &Self) -> &str {
        &self.query_time_nsec
    }
    pub fn response_time_sec(self: &Self) -> &str {
        &self.response_time_sec
    }
    pub fn response_time_nsec(self: &Self) -> &str {
        &self.response_time_nsec
    }
    pub fn dnstap_message_type(self: &Self) -> &str {
        &self.dnstap_message_type
    }
    pub fn query_message(self: &Self) -> &str {
        &self.query_message
    }
    pub fn response_message(self: &Self) -> &str {
        &self.response_message
    }
}

impl Default for DnstapMessageSchema {
    fn default() -> Self {
        Self {
            socket_family: String::from("data.socket_family"),
            socket_protocol: String::from("data.socket_protocol"),
            query_address: String::from("data.query_address"),
            query_port: String::from("data.query_port"),
            response_address: String::from("data.response_address"),
            response_port: String::from("data.response_port"),
            query_zone: String::from("data.query_zone"),
            query_time_sec: String::from("data.query_time_sec"),
            query_time_nsec: String::from("data.query_time_nsec"),
            response_time_sec: String::from("data.response_time_sec"),
            response_time_nsec: String::from("data.response_time_nsec"),
            dnstap_message_type: String::from("data.type"),
            query_message: String::from("data.query_message"),
            response_message: String::from("data.response_message"),
        }
    }
}

pub struct DnsQueryMessageSchema {
    raw_data: String,
    header: String,
    question_section: String,
    answer_section: String,
    authority_section: String,
    additional_section: String,
    opt_pseudo_section: String,
}

impl DnsQueryMessageSchema {
    pub fn raw_data(self: &Self) -> &str {
        &self.raw_data
    }
    pub fn header(self: &Self) -> &str {
        &self.header
    }
    pub fn question_section(self: &Self) -> &str {
        &self.question_section
    }
    pub fn answer_section(self: &Self) -> &str {
        &self.answer_section
    }
    pub fn authority_section(self: &Self) -> &str {
        &self.authority_section
    }
    pub fn additional_section(self: &Self) -> &str {
        &self.additional_section
    }
    pub fn opt_pseudo_section(self: &Self) -> &str {
        &self.opt_pseudo_section
    }
}

impl Default for DnsQueryMessageSchema {
    fn default() -> Self {
        Self {
            raw_data: String::from("raw_data"),
            header: String::from("header"),
            question_section: String::from("question"),
            answer_section: String::from("answer"),
            authority_section: String::from("authority"),
            additional_section: String::from("additional"),
            opt_pseudo_section: String::from("opt"),
        }
    }
}

pub struct DnsQueryHeaderSchema {
    id: String,
    opcode: String,
    rcode: String,
    qr: String,
    aa: String,
    tc: String,
    rd: String,
    ra: String,
    ad: String,
    cd: String,
    question_count: String,
    answer_count: String,
    authority_count: String,
    additional_count: String,
}

impl DnsQueryHeaderSchema {
    pub fn id(self: &Self) -> &str {
        &self.id
    }
    pub fn opcode(self: &Self) -> &str {
        &self.opcode
    }
    pub fn rcode(self: &Self) -> &str {
        &self.rcode
    }
    pub fn aa(self: &Self) -> &str {
        &self.aa
    }
    pub fn tc(self: &Self) -> &str {
        &self.tc
    }
    pub fn rd(self: &Self) -> &str {
        &self.rd
    }
    pub fn qr(self: &Self) -> &str {
        &self.qr
    }
    pub fn ra(self: &Self) -> &str {
        &self.ra
    }
    pub fn ad(self: &Self) -> &str {
        &self.ad
    }
    pub fn cd(self: &Self) -> &str {
        &self.cd
    }
    pub fn question_count(self: &Self) -> &str {
        &self.question_count
    }
    pub fn answer_count(self: &Self) -> &str {
        &self.answer_count
    }
    pub fn authority_count(self: &Self) -> &str {
        &self.authority_count
    }
    pub fn additional_count(self: &Self) -> &str {
        &self.additional_count
    }
}

impl Default for DnsQueryHeaderSchema {
    fn default() -> Self {
        Self {
            id: String::from("id"),
            opcode: String::from("opcode"),
            rcode: String::from("rcode"),
            qr: String::from("qr"),
            aa: String::from("aa"),
            tc: String::from("tc"),
            rd: String::from("rd"),
            ra: String::from("ra"),
            ad: String::from("ad"),
            cd: String::from("cd"),
            question_count: String::from("qdcount"),
            answer_count: String::from("ancount"),
            authority_count: String::from("nscount"),
            additional_count: String::from("arcount"),
        }
    }
}

pub struct DnsMessageOptPseudoSectionSchema {
    extended_rcode: String,
    version: String,
    do_flag: String,
    udp_max_payload_size: String,
    options: String,
}

impl DnsMessageOptPseudoSectionSchema {
    pub fn extended_rcode(self: &Self) -> &str {
        &self.extended_rcode
    }
    pub fn version(self: &Self) -> &str {
        &self.version
    }
    pub fn do_flag(self: &Self) -> &str {
        &self.do_flag
    }
    pub fn udp_max_payload_size(self: &Self) -> &str {
        &self.udp_max_payload_size
    }
    pub fn options(self: &Self) -> &str {
        &self.options
    }
}

impl Default for DnsMessageOptPseudoSectionSchema {
    fn default() -> Self {
        Self {
            extended_rcode: String::from("extended_rcode"),
            version: String::from("version"),
            do_flag: String::from("do"),
            udp_max_payload_size: String::from("udp_max_payload_size"),
            options: String::from("options"),
        }
    }
}

pub struct DnsMessageOptionSchema {
    opt_code: String,
    opt_name: String,
    supported_algorithms: String,
    opt_data: String,
}

impl DnsMessageOptionSchema {
    pub fn opt_code(self: &Self) -> &str {
        &self.opt_code
    }
    pub fn opt_name(self: &Self) -> &str {
        &self.opt_name
    }
    pub fn supported_algorithms(self: &Self) -> &str {
        &self.supported_algorithms
    }
    pub fn opt_data(self: &Self) -> &str {
        &self.opt_data
    }
}

impl Default for DnsMessageOptionSchema {
    fn default() -> Self {
        Self {
            opt_code: String::from("opt_code"),
            opt_name: String::from("opt_name"),
            supported_algorithms: String::from("supported_algorithms"),
            opt_data: String::from("opt_data"),
        }
    }
}

pub struct DnsRecordSchema {
    name: String,
    record_type: String,
    ttl: String,
    class: String,
    rdata: String,
}

impl DnsRecordSchema {
    pub fn name(self: &Self) -> &str {
        &self.name
    }
    pub fn record_type(self: &Self) -> &str {
        &self.record_type
    }
    pub fn ttl(self: &Self) -> &str {
        &self.ttl
    }
    pub fn class(self: &Self) -> &str {
        &self.class
    }
    pub fn rdata(self: &Self) -> &str {
        &self.rdata
    }
}

impl Default for DnsRecordSchema {
    fn default() -> Self {
        Self {
            name: String::from("name"),
            record_type: String::from("type"),
            ttl: String::from("ttl"),
            class: String::from("class"),
            rdata: String::from("rdata"),
        }
    }
}
