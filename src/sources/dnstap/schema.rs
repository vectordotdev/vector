use getset::{CopyGetters, Getters, MutGetters, Setters};

#[derive(Getters, MutGetters, Default, Debug, Clone)]
#[get = "pub"]
pub struct DnstapEventSchema {
    #[getset(get = "pub", get_mut = "pub")]
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

#[derive(CopyGetters, Setters, Debug, Clone)]
#[get_copy = "pub"]
pub struct DnstapRootDataSchema {
    server_identity: &'static str,
    server_version: &'static str,
    extra: &'static str,
    data_type: &'static str,
    data_type_id: &'static str,
    #[getset(get = "pub", set = "pub")]
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

#[derive(CopyGetters, Debug, Clone)]
#[get_copy = "pub"]
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

#[derive(CopyGetters, Debug, Clone)]
#[get_copy = "pub"]
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

#[derive(CopyGetters, Debug, Clone)]
#[get_copy = "pub"]
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

#[derive(CopyGetters, Debug, Clone)]
#[get_copy = "pub"]
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

#[derive(CopyGetters, Debug, Clone)]
#[get_copy = "pub"]
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

#[derive(CopyGetters, Debug, Clone)]
#[get_copy = "pub"]
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

#[derive(CopyGetters, Debug, Clone)]
#[get_copy = "pub"]
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

#[derive(CopyGetters, Debug, Clone)]
#[get_copy = "pub"]
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

#[derive(CopyGetters, Debug, Clone)]
#[get_copy = "pub"]
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

#[derive(CopyGetters, Debug, Clone)]
#[get_copy = "pub"]
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

#[derive(CopyGetters, Debug, Clone)]
#[get_copy = "pub"]
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

#[derive(CopyGetters, Debug, Clone)]
#[get_copy = "pub"]
pub struct DnsUpdateZoneInfoSchema {
    zone_name: &'static str,
    zone_class: &'static str,
    zone_type: &'static str,
    zone_type_id: &'static str,
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
