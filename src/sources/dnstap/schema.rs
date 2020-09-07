#[readonly::make]
#[derive(Default, Debug, Clone)]
pub struct DnstapEventSchema {
    pub dnstap_root_data_schema: DnstapRootDataSchema,
    pub dnstap_message_schema: DnstapMessageSchema,
    pub dns_query_message_schema: DnsQueryMessageSchema,
    pub dns_query_header_schema: DnsQueryHeaderSchema,
    pub dns_update_message_schema: DnsUpdateMessageSchema,
    pub dns_update_header_schema: DnsUpdateHeaderSchema,
    pub dns_message_opt_pseudo_section_schema: DnsMessageOptPseudoSectionSchema,
    pub dns_message_option_schema: DnsMessageOptionSchema,
    pub dns_record_schema: DnsRecordSchema,
    pub dns_query_question_schema: DnsQueryQuestionSchema,
    pub dns_update_zone_info_schema: DnsUpdateZoneInfoSchema,
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

#[readonly::make]
#[derive(Debug, Clone)]
pub struct DnstapRootDataSchema {
    pub server_identity: &'static str,
    pub server_version: &'static str,
    pub extra: &'static str,
    pub data_type: &'static str,
    pub data_type_id: &'static str,
    pub timestamp: &'static str,
    pub time_precision: &'static str,
    pub error: &'static str,
    pub raw_data: &'static str,
}

impl Default for DnstapRootDataSchema {
    fn default() -> Self {
        Self {
            server_identity: "serverId",
            server_version: "serverVersion",
            extra: "extraInfo",
            data_type: "dataType",
            data_type_id: "dataTypeId",
            timestamp: "time",
            time_precision: "timePrecision",
            error: "error",
            raw_data: "rawData",
        }
    }
}

#[readonly::make]
#[derive(Debug, Clone)]
pub struct DnstapMessageSchema {
    pub socket_family: &'static str,
    pub socket_protocol: &'static str,
    pub query_address: &'static str,
    pub query_port: &'static str,
    pub response_address: &'static str,
    pub response_port: &'static str,
    pub query_zone: &'static str,
    pub dnstap_message_type: &'static str,
    pub dnstap_message_type_id: &'static str,
    pub request_message: &'static str,
    pub response_message: &'static str,
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

#[readonly::make]
#[derive(Debug, Clone)]
struct DnsMessageCommonSchema {
    pub response_code: &'static str,
    pub response: &'static str,
    pub timestamp: &'static str,
    pub time_precision: &'static str,
    pub raw_data: &'static str,
    pub header: &'static str,
}

impl Default for DnsMessageCommonSchema {
    fn default() -> Self {
        Self {
            response_code: "fullRcode",
            response: "rcodeName",
            timestamp: "time",
            time_precision: "timePrecision",
            raw_data: "rawData",
            header: "header",
        }
    }
}

#[readonly::make]
#[derive(Debug, Clone)]
pub struct DnsQueryMessageSchema {
    pub response_code: &'static str,
    pub response: &'static str,
    pub timestamp: &'static str,
    pub time_precision: &'static str,
    pub raw_data: &'static str,
    pub header: &'static str,
    pub question_section: &'static str,
    pub answer_section: &'static str,
    pub authority_section: &'static str,
    pub additional_section: &'static str,
    pub opt_pseudo_section: &'static str,
}

impl Default for DnsQueryMessageSchema {
    fn default() -> Self {
        let common_schema = DnsMessageCommonSchema::default();
        Self {
            response_code: common_schema.response_code,
            response: common_schema.response,
            timestamp: common_schema.timestamp,
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

#[readonly::make]
#[derive(Debug, Clone)]
pub struct DnsUpdateMessageSchema {
    pub response_code: &'static str,
    pub response: &'static str,
    pub timestamp: &'static str,
    pub time_precision: &'static str,
    pub raw_data: &'static str,
    pub header: &'static str,
    pub zone_section: &'static str,
    pub prerequisite_section: &'static str,
    pub update_section: &'static str,
    pub additional_section: &'static str,
}

impl Default for DnsUpdateMessageSchema {
    fn default() -> Self {
        let common_schema = DnsMessageCommonSchema::default();
        Self {
            response_code: common_schema.response_code,
            response: common_schema.response,
            timestamp: common_schema.timestamp,
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

#[readonly::make]
#[derive(Debug, Clone)]
struct DnsMessageHeaderCommonSchema {
    pub id: &'static str,
    pub opcode: &'static str,
    pub rcode: &'static str,
    pub qr: &'static str,
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

#[readonly::make]
#[derive(Debug, Clone)]
pub struct DnsQueryHeaderSchema {
    pub id: &'static str,
    pub opcode: &'static str,
    pub rcode: &'static str,
    pub qr: &'static str,
    pub aa: &'static str,
    pub tc: &'static str,
    pub rd: &'static str,
    pub ra: &'static str,
    pub ad: &'static str,
    pub cd: &'static str,
    pub question_count: &'static str,
    pub answer_count: &'static str,
    pub authority_count: &'static str,
    pub additional_count: &'static str,
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

#[readonly::make]
#[derive(Debug, Clone)]
pub struct DnsUpdateHeaderSchema {
    pub id: &'static str,
    pub opcode: &'static str,
    pub rcode: &'static str,
    pub qr: &'static str,
    pub zone_count: &'static str,
    pub prerequisite_count: &'static str,
    pub udpate_count: &'static str,
    pub additional_count: &'static str,
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
            udpate_count: "upCount",
            additional_count: "adCount",
        }
    }
}

#[readonly::make]
#[derive(Debug, Clone)]
pub struct DnsMessageOptPseudoSectionSchema {
    pub extended_rcode: &'static str,
    pub version: &'static str,
    pub do_flag: &'static str,
    pub udp_max_payload_size: &'static str,
    pub options: &'static str,
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

#[readonly::make]
#[derive(Debug, Clone)]
pub struct DnsMessageOptionSchema {
    pub opt_code: &'static str,
    pub opt_name: &'static str,
    pub opt_data: &'static str,
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

#[readonly::make]
#[derive(Debug, Clone)]
pub struct DnsRecordSchema {
    pub name: &'static str,
    pub record_type: &'static str,
    pub record_type_id: &'static str,
    pub ttl: &'static str,
    pub class: &'static str,
    pub rdata: &'static str,
    pub rdata_bytes: &'static str,
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

#[readonly::make]
#[derive(Debug, Clone)]
pub struct DnsQueryQuestionSchema {
    pub name: &'static str,
    pub question_type: &'static str,
    pub question_type_id: &'static str,
    pub class: &'static str,
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

#[readonly::make]
#[derive(Debug, Clone)]
pub struct DnsUpdateZoneInfoSchema {
    pub zone_name: &'static str,
    pub zone_class: &'static str,
    pub zone_type: &'static str,
    pub zone_type_id: &'static str,
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
