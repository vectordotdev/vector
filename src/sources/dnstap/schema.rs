#[derive(Default, Debug, Clone)]
pub struct DnstapEventSchema {
    dnstap_root_data_schema: DnstapRootDataSchema,
    dnstap_message_schema: DnstapMessageSchema,
    dns_query_message_schema: DnsQueryMessageSchema,
    dns_query_header_schema: DnsQueryHeaderSchema,
    dns_update_message_schema: DnsUpdateMessageSchema,
    dns_update_header_schema: DnsUpdateHeaderSchema,
    dns_message_opt_pseudo_section_schema: DnsMessageOptPseudoSectionSchema,
    dns_message_option_schema: DnsMessageOptionSchema,
    dns_record_schema: DnsRecordSchema,
    dns_query_question_schema: DnsQueryQuestionSchema,
    dns_update_zone_info_schema: DnsUpdateZoneInfoSchema,
}

impl DnstapEventSchema {
    pub const fn dnstap_root_data_schema(&self) -> &DnstapRootDataSchema {
        &self.dnstap_root_data_schema
    }

    pub const fn dnstap_message_schema(&self) -> &DnstapMessageSchema {
        &self.dnstap_message_schema
    }

    pub const fn dns_query_message_schema(&self) -> &DnsQueryMessageSchema {
        &self.dns_query_message_schema
    }

    pub const fn dns_query_header_schema(&self) -> &DnsQueryHeaderSchema {
        &self.dns_query_header_schema
    }

    pub const fn dns_update_message_schema(&self) -> &DnsUpdateMessageSchema {
        &self.dns_update_message_schema
    }

    pub const fn dns_update_header_schema(&self) -> &DnsUpdateHeaderSchema {
        &self.dns_update_header_schema
    }

    pub const fn dns_message_opt_pseudo_section_schema(&self) -> &DnsMessageOptPseudoSectionSchema {
        &self.dns_message_opt_pseudo_section_schema
    }

    pub const fn dns_message_option_schema(&self) -> &DnsMessageOptionSchema {
        &self.dns_message_option_schema
    }

    pub const fn dns_record_schema(&self) -> &DnsRecordSchema {
        &self.dns_record_schema
    }

    pub const fn dns_query_question_schema(&self) -> &DnsQueryQuestionSchema {
        &self.dns_query_question_schema
    }

    pub const fn dns_update_zone_info_schema(&self) -> &DnsUpdateZoneInfoSchema {
        &self.dns_update_zone_info_schema
    }

    pub fn dnstap_root_data_schema_mut(&mut self) -> &mut DnstapRootDataSchema {
        &mut self.dnstap_root_data_schema
    }

    pub fn new() -> Self {
        Self {
            dnstap_root_data_schema: DnstapRootDataSchema::default(),
            dnstap_message_schema: DnstapMessageSchema::default(),
            dns_query_message_schema: DnsQueryMessageSchema::default(),
            dns_query_header_schema: DnsQueryHeaderSchema::default(),
            dns_update_message_schema: DnsUpdateMessageSchema::default(),
            dns_update_header_schema: DnsUpdateHeaderSchema::default(),
            dns_message_opt_pseudo_section_schema: DnsMessageOptPseudoSectionSchema::default(),
            dns_message_option_schema: DnsMessageOptionSchema::default(),
            dns_record_schema: DnsRecordSchema::default(),
            dns_query_question_schema: DnsQueryQuestionSchema::default(),
            dns_update_zone_info_schema: DnsUpdateZoneInfoSchema::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DnstapRootDataSchema {
    server_identity: &'static str,
    server_version: &'static str,
    extra: &'static str,
    data_type: &'static str,
    data_type_id: &'static str,
    timestamp: &'static str,
    time: &'static str,
    time_precision: &'static str,
    error: &'static str,
    raw_data: &'static str,
}

impl Default for DnstapRootDataSchema {
    fn default() -> Self {
        Self {
            server_identity: "serverId",
            server_version: "serverVersion",
            extra: "extraInfo",
            data_type: "dataType",
            data_type_id: "dataTypeId",
            timestamp: "timestamp",
            time: "time",
            time_precision: "timePrecision",
            error: "error",
            raw_data: "rawData",
        }
    }
}

impl DnstapRootDataSchema {
    pub const fn server_identity(&self) -> &'static str {
        self.server_identity
    }

    pub const fn server_version(&self) -> &'static str {
        self.server_version
    }

    pub const fn extra(&self) -> &'static str {
        self.extra
    }

    pub const fn data_type(&self) -> &'static str {
        self.data_type
    }

    pub const fn data_type_id(&self) -> &'static str {
        self.data_type_id
    }

    pub const fn timestamp(&self) -> &'static str {
        self.timestamp
    }

    pub const fn time(&self) -> &'static str {
        self.time
    }

    pub const fn time_precision(&self) -> &'static str {
        self.time_precision
    }

    pub const fn error(&self) -> &'static str {
        self.error
    }

    pub const fn raw_data(&self) -> &'static str {
        self.raw_data
    }

    pub fn set_timestamp(&mut self, val: &'static str) -> &mut Self {
        self.timestamp = val;
        self
    }
}

#[derive(Debug, Clone)]
pub struct DnstapMessageSchema {
    socket_family: &'static str,
    socket_protocol: &'static str,
    query_address: &'static str,
    query_port: &'static str,
    response_address: &'static str,
    response_port: &'static str,
    query_zone: &'static str,
    dnstap_message_type: &'static str,
    dnstap_message_type_id: &'static str,
    request_message: &'static str,
    response_message: &'static str,
}

impl Default for DnstapMessageSchema {
    fn default() -> Self {
        Self {
            socket_family: "socketFamily",
            socket_protocol: "socketProtocol",
            query_address: "sourceAddress",
            query_port: "sourcePort",
            response_address: "responseAddress",
            response_port: "responsePort",
            query_zone: "queryZone",
            dnstap_message_type: "messageType",
            dnstap_message_type_id: "messageTypeId",
            request_message: "requestData",
            response_message: "responseData",
        }
    }
}

impl DnstapMessageSchema {
    pub const fn socket_family(&self) -> &'static str {
        self.socket_family
    }

    pub const fn socket_protocol(&self) -> &'static str {
        self.socket_protocol
    }

    pub const fn query_address(&self) -> &'static str {
        self.query_address
    }

    pub const fn query_port(&self) -> &'static str {
        self.query_port
    }

    pub const fn response_address(&self) -> &'static str {
        self.response_address
    }

    pub const fn response_port(&self) -> &'static str {
        self.response_port
    }

    pub const fn query_zone(&self) -> &'static str {
        self.query_zone
    }

    pub const fn dnstap_message_type(&self) -> &'static str {
        self.dnstap_message_type
    }

    pub const fn dnstap_message_type_id(&self) -> &'static str {
        self.dnstap_message_type_id
    }

    pub const fn request_message(&self) -> &'static str {
        self.request_message
    }

    pub const fn response_message(&self) -> &'static str {
        self.response_message
    }
}

#[derive(Debug, Clone)]
pub struct DnsMessageCommonSchema {
    response_code: &'static str,
    response: &'static str,
    time: &'static str,
    time_precision: &'static str,
    raw_data: &'static str,
    header: &'static str,
}

impl Default for DnsMessageCommonSchema {
    fn default() -> Self {
        Self {
            response_code: "fullRcode",
            response: "rcodeName",
            time: "time",
            time_precision: "timePrecision",
            raw_data: "rawData",
            header: "header",
        }
    }
}

impl DnsMessageCommonSchema {
    pub const fn response_code(&self) -> &'static str {
        self.response_code
    }

    pub const fn response(&self) -> &'static str {
        self.response
    }

    pub const fn time(&self) -> &'static str {
        self.time
    }

    pub const fn time_precision(&self) -> &'static str {
        self.time_precision
    }

    pub const fn raw_data(&self) -> &'static str {
        self.raw_data
    }

    pub const fn header(&self) -> &'static str {
        self.header
    }
}

#[derive(Debug, Clone)]
pub struct DnsQueryMessageSchema {
    response_code: &'static str,
    response: &'static str,
    time: &'static str,
    time_precision: &'static str,
    raw_data: &'static str,
    header: &'static str,
    question_section: &'static str,
    answer_section: &'static str,
    authority_section: &'static str,
    additional_section: &'static str,
    opt_pseudo_section: &'static str,
}

impl Default for DnsQueryMessageSchema {
    fn default() -> Self {
        let common_schema = DnsMessageCommonSchema::default();
        Self {
            response_code: common_schema.response_code,
            response: common_schema.response,
            time: common_schema.time,
            time_precision: common_schema.time_precision,
            raw_data: common_schema.raw_data,
            header: common_schema.header,
            question_section: "question",
            answer_section: "answers",
            authority_section: "authority",
            additional_section: "additional",
            opt_pseudo_section: "opt",
        }
    }
}

impl DnsQueryMessageSchema {
    pub const fn response_code(&self) -> &'static str {
        self.response_code
    }

    pub const fn response(&self) -> &'static str {
        self.response
    }

    pub const fn time(&self) -> &'static str {
        self.time
    }

    pub const fn time_precision(&self) -> &'static str {
        self.time_precision
    }

    pub const fn raw_data(&self) -> &'static str {
        self.raw_data
    }

    pub const fn header(&self) -> &'static str {
        self.header
    }

    pub const fn question_section(&self) -> &'static str {
        self.question_section
    }

    pub const fn answer_section(&self) -> &'static str {
        self.answer_section
    }

    pub const fn authority_section(&self) -> &'static str {
        self.authority_section
    }

    pub const fn additional_section(&self) -> &'static str {
        self.additional_section
    }

    pub const fn opt_pseudo_section(&self) -> &'static str {
        self.opt_pseudo_section
    }
}

#[derive(Debug, Clone)]
pub struct DnsUpdateMessageSchema {
    response_code: &'static str,
    response: &'static str,
    time: &'static str,
    time_precision: &'static str,
    raw_data: &'static str,
    header: &'static str,
    zone_section: &'static str,
    prerequisite_section: &'static str,
    update_section: &'static str,
    additional_section: &'static str,
}

impl Default for DnsUpdateMessageSchema {
    fn default() -> Self {
        let common_schema = DnsMessageCommonSchema::default();
        Self {
            response_code: common_schema.response_code,
            response: common_schema.response,
            time: common_schema.time,
            time_precision: common_schema.time_precision,
            raw_data: common_schema.raw_data,
            header: common_schema.header,
            zone_section: "zone",
            prerequisite_section: "prerequisite",
            update_section: "update",
            additional_section: "additional",
        }
    }
}

impl DnsUpdateMessageSchema {
    pub const fn response_code(&self) -> &'static str {
        self.response_code
    }

    pub const fn response(&self) -> &'static str {
        self.response
    }

    pub const fn time(&self) -> &'static str {
        self.time
    }

    pub const fn time_precision(&self) -> &'static str {
        self.time_precision
    }

    pub const fn raw_data(&self) -> &'static str {
        self.raw_data
    }

    pub const fn header(&self) -> &'static str {
        self.header
    }

    pub const fn zone_section(&self) -> &'static str {
        self.zone_section
    }

    pub const fn prerequisite_section(&self) -> &'static str {
        self.prerequisite_section
    }

    pub const fn update_section(&self) -> &'static str {
        self.update_section
    }

    pub const fn additional_section(&self) -> &'static str {
        self.additional_section
    }
}

#[derive(Debug, Clone)]
pub struct DnsMessageHeaderCommonSchema {
    id: &'static str,
    opcode: &'static str,
    rcode: &'static str,
    qr: &'static str,
}

impl Default for DnsMessageHeaderCommonSchema {
    fn default() -> Self {
        Self {
            id: "id",
            opcode: "opcode",
            rcode: "rcode",
            qr: "qr",
        }
    }
}

impl DnsMessageHeaderCommonSchema {
    pub const fn id(&self) -> &'static str {
        self.id
    }

    pub const fn opcode(&self) -> &'static str {
        self.opcode
    }

    pub const fn rcode(&self) -> &'static str {
        self.rcode
    }

    pub const fn qr(&self) -> &'static str {
        self.qr
    }
}

impl DnsQueryHeaderSchema {
    pub const fn id(&self) -> &'static str {
        self.id
    }

    pub const fn opcode(&self) -> &'static str {
        self.opcode
    }

    pub const fn rcode(&self) -> &'static str {
        self.rcode
    }

    pub const fn qr(&self) -> &'static str {
        self.qr
    }

    pub const fn aa(&self) -> &'static str {
        self.aa
    }

    pub const fn tc(&self) -> &'static str {
        self.tc
    }

    pub const fn rd(&self) -> &'static str {
        self.rd
    }

    pub const fn ra(&self) -> &'static str {
        self.ra
    }

    pub const fn ad(&self) -> &'static str {
        self.ad
    }

    pub const fn cd(&self) -> &'static str {
        self.cd
    }

    pub const fn question_count(&self) -> &'static str {
        self.question_count
    }

    pub const fn answer_count(&self) -> &'static str {
        self.answer_count
    }

    pub const fn authority_count(&self) -> &'static str {
        self.authority_count
    }

    pub const fn additional_count(&self) -> &'static str {
        self.additional_count
    }
}

#[derive(Debug, Clone)]
pub struct DnsQueryHeaderSchema {
    id: &'static str,
    opcode: &'static str,
    rcode: &'static str,
    qr: &'static str,
    aa: &'static str,
    tc: &'static str,
    rd: &'static str,
    ra: &'static str,
    ad: &'static str,
    cd: &'static str,
    question_count: &'static str,
    answer_count: &'static str,
    authority_count: &'static str,
    additional_count: &'static str,
}

impl Default for DnsQueryHeaderSchema {
    fn default() -> Self {
        let header_common_schema = DnsMessageHeaderCommonSchema::default();
        Self {
            id: header_common_schema.id,
            opcode: header_common_schema.opcode,
            rcode: header_common_schema.rcode,
            qr: header_common_schema.qr,
            aa: "aa",
            tc: "tc",
            rd: "rd",
            ra: "ra",
            ad: "ad",
            cd: "cd",
            question_count: "qdCount",
            answer_count: "anCount",
            authority_count: "nsCount",
            additional_count: "arCount",
        }
    }
}

impl DnsUpdateHeaderSchema {
    pub const fn id(&self) -> &'static str {
        self.id
    }

    pub const fn opcode(&self) -> &'static str {
        self.opcode
    }

    pub const fn rcode(&self) -> &'static str {
        self.rcode
    }

    pub const fn qr(&self) -> &'static str {
        self.qr
    }

    pub const fn zone_count(&self) -> &'static str {
        self.zone_count
    }

    pub const fn prerequisite_count(&self) -> &'static str {
        self.prerequisite_count
    }

    pub const fn update_count(&self) -> &'static str {
        self.update_count
    }

    pub const fn additional_count(&self) -> &'static str {
        self.additional_count
    }
}

#[derive(Debug, Clone)]
pub struct DnsUpdateHeaderSchema {
    id: &'static str,
    opcode: &'static str,
    rcode: &'static str,
    qr: &'static str,
    zone_count: &'static str,
    prerequisite_count: &'static str,
    update_count: &'static str,
    additional_count: &'static str,
}

impl Default for DnsUpdateHeaderSchema {
    fn default() -> Self {
        let header_common_schema = DnsMessageHeaderCommonSchema::default();
        Self {
            id: header_common_schema.id,
            opcode: header_common_schema.opcode,
            rcode: header_common_schema.rcode,
            qr: header_common_schema.qr,
            zone_count: "zoCount",
            prerequisite_count: "prCount",
            update_count: "upCount",
            additional_count: "adCount",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DnsMessageOptPseudoSectionSchema {
    extended_rcode: &'static str,
    version: &'static str,
    do_flag: &'static str,
    udp_max_payload_size: &'static str,
    options: &'static str,
}

impl Default for DnsMessageOptPseudoSectionSchema {
    fn default() -> Self {
        Self {
            extended_rcode: "extendedRcode",
            version: "ednsVersion",
            do_flag: "do",
            udp_max_payload_size: "udpPayloadSize",
            options: "options",
        }
    }
}

impl DnsMessageOptPseudoSectionSchema {
    pub const fn extended_rcode(&self) -> &'static str {
        self.extended_rcode
    }

    pub const fn version(&self) -> &'static str {
        self.version
    }

    pub const fn do_flag(&self) -> &'static str {
        self.do_flag
    }

    pub const fn udp_max_payload_size(&self) -> &'static str {
        self.udp_max_payload_size
    }

    pub const fn options(&self) -> &'static str {
        self.options
    }
}

#[derive(Debug, Clone)]
pub struct DnsMessageOptionSchema {
    opt_code: &'static str,
    opt_name: &'static str,
    opt_data: &'static str,
}

impl Default for DnsMessageOptionSchema {
    fn default() -> Self {
        Self {
            opt_code: "optCode",
            opt_name: "optName",
            opt_data: "optValue",
        }
    }
}

impl DnsMessageOptionSchema {
    pub const fn opt_code(&self) -> &'static str {
        self.opt_code
    }

    pub const fn opt_name(&self) -> &'static str {
        self.opt_name
    }

    pub const fn opt_data(&self) -> &'static str {
        self.opt_data
    }
}

#[derive(Debug, Clone)]
pub struct DnsRecordSchema {
    name: &'static str,
    record_type: &'static str,
    record_type_id: &'static str,
    ttl: &'static str,
    class: &'static str,
    rdata: &'static str,
    rdata_bytes: &'static str,
}

impl Default for DnsRecordSchema {
    fn default() -> Self {
        Self {
            name: "domainName",
            record_type: "recordType",
            record_type_id: "recordTypeId",
            ttl: "ttl",
            class: "class",
            rdata: "rData",
            rdata_bytes: "rDataBytes",
        }
    }
}

impl DnsRecordSchema {
    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub const fn record_type(&self) -> &'static str {
        self.record_type
    }

    pub const fn record_type_id(&self) -> &'static str {
        self.record_type_id
    }

    pub const fn ttl(&self) -> &'static str {
        self.ttl
    }

    pub const fn class(&self) -> &'static str {
        self.class
    }

    pub const fn rdata(&self) -> &'static str {
        self.rdata
    }

    pub const fn rdata_bytes(&self) -> &'static str {
        self.rdata_bytes
    }
}

#[derive(Debug, Clone)]
pub struct DnsQueryQuestionSchema {
    name: &'static str,
    question_type: &'static str,
    question_type_id: &'static str,
    class: &'static str,
}

impl Default for DnsQueryQuestionSchema {
    fn default() -> Self {
        Self {
            name: "domainName",
            question_type: "questionType",
            question_type_id: "questionTypeId",
            class: "class",
        }
    }
}

impl DnsQueryQuestionSchema {
    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub const fn question_type(&self) -> &'static str {
        self.question_type
    }

    pub const fn question_type_id(&self) -> &'static str {
        self.question_type_id
    }

    pub const fn class(&self) -> &'static str {
        self.class
    }
}

#[derive(Debug, Clone)]
pub struct DnsUpdateZoneInfoSchema {
    zone_name: &'static str,
    zone_class: &'static str,
    zone_type: &'static str,
    zone_type_id: &'static str,
}

impl DnsUpdateZoneInfoSchema {
    pub const fn zone_name(&self) -> &'static str {
        self.zone_name
    }

    pub const fn zone_class(&self) -> &'static str {
        self.zone_class
    }

    pub const fn zone_type(&self) -> &'static str {
        self.zone_type
    }

    pub const fn zone_type_id(&self) -> &'static str {
        self.zone_type_id
    }
}

impl Default for DnsUpdateZoneInfoSchema {
    fn default() -> Self {
        Self {
            zone_name: "zName",
            zone_class: "zClass",
            zone_type: "zType",
            zone_type_id: "zTypeId",
        }
    }
}
