use once_cell::sync::Lazy;
use std::collections::BTreeMap;
use vector_lib::lookup::{owned_value_path, OwnedValuePath};
use vrl::btreemap;
use vrl::value::{
    kind::{Collection, Field},
    Kind,
};

#[derive(Debug, Default, Clone)]
pub struct DnstapEventSchema;

impl DnstapEventSchema {
    /// The message schema for the request and response message fields
    fn request_message_schema_definition(&self) -> Collection<Field> {
        let mut result: BTreeMap<Field, Kind> = BTreeMap::new();
        result.insert(
            DNSTAP_VALUE_PATHS.time.to_string().into(),
            Kind::integer().or_undefined(),
        );

        result.insert(
            DNSTAP_VALUE_PATHS.time.to_string().into(),
            Kind::integer().or_undefined(),
        );
        result.insert(
            DNSTAP_VALUE_PATHS.time_precision.to_string().into(),
            Kind::bytes().or_undefined(),
        );
        result.insert(
            DNSTAP_VALUE_PATHS.time_precision.to_string().into(),
            Kind::bytes().or_undefined(),
        );
        result.insert(
            DNSTAP_VALUE_PATHS.response_code.to_string().into(),
            Kind::integer().or_undefined(),
        );
        result.insert(
            DNSTAP_VALUE_PATHS.response_code.to_string().into(),
            Kind::integer().or_undefined(),
        );
        result.insert(
            DNSTAP_VALUE_PATHS.response.to_string().into(),
            Kind::bytes().or_undefined(),
        );
        result.insert(
            DNSTAP_VALUE_PATHS.response.to_string().into(),
            Kind::bytes().or_undefined(),
        );

        let mut schema = DnsQueryHeaderSchema::schema_definition();
        schema.merge(DnsUpdateHeaderSchema::schema_definition(), true);

        result.insert(
            DNSTAP_VALUE_PATHS.header.to_string().into(),
            Kind::object(schema).or_undefined(),
        );

        result.insert(
            DNSTAP_VALUE_PATHS.zone_section.to_string().into(),
            Kind::object(DnsUpdateZoneInfoSchema::schema_definition()).or_undefined(),
        );

        result.insert(
            DNSTAP_VALUE_PATHS.question_section.to_string().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                DnsQueryQuestionSchema::schema_definition(),
            )))
            .or_undefined(),
        );

        result.insert(
            DNSTAP_VALUE_PATHS.answer_section.to_string().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                DnsRecordSchema::schema_definition(),
            )))
            .or_undefined(),
        );

        result.insert(
            DNSTAP_VALUE_PATHS.authority_section.to_string().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                DnsRecordSchema::schema_definition(),
            )))
            .or_undefined(),
        );

        result.insert(
            DNSTAP_VALUE_PATHS.additional_section.to_string().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                DnsRecordSchema::schema_definition(),
            )))
            .or_undefined(),
        );

        result.insert(
            DNSTAP_VALUE_PATHS.opt_pseudo_section.to_string().into(),
            Kind::object(DnsMessageOptPseudoSectionSchema::schema_definition()).or_undefined(),
        );

        result.insert(
            DNSTAP_VALUE_PATHS.raw_data.to_string().into(),
            Kind::bytes().or_undefined(),
        );

        result.insert(
            DNSTAP_VALUE_PATHS.prerequisite_section.to_string().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                DnsRecordSchema::schema_definition(),
            )))
            .or_undefined(),
        );

        result.insert(
            DNSTAP_VALUE_PATHS.update_section.to_string().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                DnsRecordSchema::schema_definition(),
            )))
            .or_undefined(),
        );

        result.insert(
            DNSTAP_VALUE_PATHS.additional_section.to_string().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                DnsRecordSchema::schema_definition(),
            )))
            .or_undefined(),
        );

        result.into()
    }

    /// Schema definition for fields stored in the root.
    fn root_schema_definition(
        &self,
        schema: vector_lib::schema::Definition,
    ) -> vector_lib::schema::Definition {
        schema
            .optional_field(&DNSTAP_VALUE_PATHS.server_identity, Kind::bytes(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.server_version, Kind::bytes(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.extra, Kind::bytes(), None)
            .with_event_field(&DNSTAP_VALUE_PATHS.data_type_id, Kind::integer(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.data_type, Kind::bytes(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.error, Kind::bytes(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.raw_data, Kind::bytes(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.time, Kind::integer(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.time_precision, Kind::bytes(), None)
    }

    /// Schema definition from the message.
    pub fn message_schema_definition(
        &self,
        schema: vector_lib::schema::Definition,
    ) -> vector_lib::schema::Definition {
        schema
            .optional_field(&DNSTAP_VALUE_PATHS.socket_family, Kind::bytes(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.socket_protocol, Kind::bytes(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.query_address, Kind::bytes(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.query_port, Kind::integer(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.response_address, Kind::bytes(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.response_port, Kind::integer(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.query_zone, Kind::bytes(), None)
            .with_event_field(&DNSTAP_VALUE_PATHS.message_type_id, Kind::integer(), None)
            .optional_field(&DNSTAP_VALUE_PATHS.message_type, Kind::bytes(), None)
            .optional_field(
                &DNSTAP_VALUE_PATHS.request_message,
                Kind::object(self.request_message_schema_definition()),
                None,
            )
            .optional_field(
                &DNSTAP_VALUE_PATHS.response_message,
                Kind::object(self.request_message_schema_definition()),
                None,
            )
    }

    /// The schema definition for a dns tap message.
    pub fn schema_definition(
        &self,
        schema: vector_lib::schema::Definition,
    ) -> vector_lib::schema::Definition {
        self.root_schema_definition(self.message_schema_definition(schema))
    }
}

/// Collection of owned value paths.
#[derive(Debug, Clone)]
pub struct DnstapPaths {
    // DnstapRootDataSchema
    pub server_identity: OwnedValuePath,
    pub server_version: OwnedValuePath,
    pub extra: OwnedValuePath,
    pub data_type: OwnedValuePath,
    pub data_type_id: OwnedValuePath,
    pub time: OwnedValuePath,
    pub time_precision: OwnedValuePath,
    pub error: OwnedValuePath,
    pub raw_data: OwnedValuePath,

    // DnstapMessageSchema
    pub socket_family: OwnedValuePath,
    pub socket_protocol: OwnedValuePath,
    pub query_address: OwnedValuePath,
    pub query_port: OwnedValuePath,
    pub response_address: OwnedValuePath,
    pub response_port: OwnedValuePath,
    pub query_zone: OwnedValuePath,
    pub message_type: OwnedValuePath,
    pub message_type_id: OwnedValuePath,
    pub request_message: OwnedValuePath,
    pub response_message: OwnedValuePath,

    // DnsQueryMessageSchema
    pub response_code: OwnedValuePath,
    pub response: OwnedValuePath,
    pub header: OwnedValuePath,
    pub question_section: OwnedValuePath,
    pub answer_section: OwnedValuePath,
    pub authority_section: OwnedValuePath,
    pub additional_section: OwnedValuePath,
    pub opt_pseudo_section: OwnedValuePath,

    // DnsUpdateMessageSchema
    pub zone_section: OwnedValuePath,
    pub prerequisite_section: OwnedValuePath,
    pub update_section: OwnedValuePath,

    // DnsMessageHeaderCommonSchema
    pub id: OwnedValuePath,
    pub opcode: OwnedValuePath,
    pub rcode: OwnedValuePath,
    pub qr: OwnedValuePath,

    // DnsQueryHeaderSchema
    pub aa: OwnedValuePath,
    pub tc: OwnedValuePath,
    pub rd: OwnedValuePath,
    pub ra: OwnedValuePath,
    pub ad: OwnedValuePath,
    pub cd: OwnedValuePath,
    pub question_count: OwnedValuePath,
    pub answer_count: OwnedValuePath,
    pub authority_count: OwnedValuePath,
    pub ar_count: OwnedValuePath,

    // DnsUpdateHeaderSchema
    pub zone_count: OwnedValuePath,
    pub prerequisite_count: OwnedValuePath,
    pub update_count: OwnedValuePath,
    pub ad_count: OwnedValuePath,

    // DnsMessageOptPseudoSectionSchema
    pub extended_rcode: OwnedValuePath,
    pub version: OwnedValuePath,
    pub do_flag: OwnedValuePath,
    pub udp_max_payload_size: OwnedValuePath,
    pub ede: OwnedValuePath,
    pub options: OwnedValuePath,

    // DnsMessageEdeOptionSchema
    pub info_code: OwnedValuePath,
    pub purpose: OwnedValuePath,
    pub extra_text: OwnedValuePath,

    // DnsMessageOptionSchema
    pub opt_code: OwnedValuePath,
    pub opt_name: OwnedValuePath,
    pub opt_data: OwnedValuePath,

    // DnsRecordSchema
    pub domain_name: OwnedValuePath,
    pub record_type: OwnedValuePath,
    pub record_type_id: OwnedValuePath,
    pub ttl: OwnedValuePath,
    pub class: OwnedValuePath,
    pub rdata: OwnedValuePath,
    pub rdata_bytes: OwnedValuePath,

    // DnsQueryQuestionSchema
    pub question_type: OwnedValuePath,
    pub question_type_id: OwnedValuePath,

    // DnsUpdateZoneInfoSchema
    pub zone_name: OwnedValuePath,
    pub zone_class: OwnedValuePath,
    pub zone_type: OwnedValuePath,
    pub zone_type_id: OwnedValuePath,
}

/// Lazily initialized singleton.
pub(crate) static DNSTAP_VALUE_PATHS: Lazy<DnstapPaths> = Lazy::new(|| DnstapPaths {
    server_identity: owned_value_path!("serverId"),
    server_version: owned_value_path!("serverVersion"),
    extra: owned_value_path!("extraInfo"),
    data_type: owned_value_path!("dataType"),
    data_type_id: owned_value_path!("dataTypeId"),
    time: owned_value_path!("time"),
    time_precision: owned_value_path!("timePrecision"),
    error: owned_value_path!("error"),
    raw_data: owned_value_path!("rawData"),
    socket_family: owned_value_path!("socketFamily"),
    socket_protocol: owned_value_path!("socketProtocol"),
    query_address: owned_value_path!("sourceAddress"),
    query_port: owned_value_path!("sourcePort"),
    response_address: owned_value_path!("responseAddress"),
    response_port: owned_value_path!("responsePort"),
    query_zone: owned_value_path!("queryZone"),
    message_type: owned_value_path!("messageType"),
    message_type_id: owned_value_path!("messageTypeId"),
    request_message: owned_value_path!("requestData"),
    response_message: owned_value_path!("responseData"),
    response_code: owned_value_path!("fullRcode"),
    response: owned_value_path!("rcodeName"),
    header: owned_value_path!("header"),
    question_section: owned_value_path!("question"),
    answer_section: owned_value_path!("answers"),
    authority_section: owned_value_path!("authority"),
    additional_section: owned_value_path!("additional"),
    opt_pseudo_section: owned_value_path!("opt"),
    zone_section: owned_value_path!("zone"),
    prerequisite_section: owned_value_path!("prerequisite"),
    update_section: owned_value_path!("update"),
    id: owned_value_path!("id"),
    opcode: owned_value_path!("opcode"),
    rcode: owned_value_path!("rcode"),
    qr: owned_value_path!("qr"),
    aa: owned_value_path!("aa"),
    tc: owned_value_path!("tc"),
    rd: owned_value_path!("rd"),
    ra: owned_value_path!("ra"),
    ad: owned_value_path!("ad"),
    cd: owned_value_path!("cd"),
    question_count: owned_value_path!("qdCount"),
    answer_count: owned_value_path!("anCount"),
    authority_count: owned_value_path!("nsCount"),
    ar_count: owned_value_path!("arCount"),
    zone_count: owned_value_path!("zoCount"),
    prerequisite_count: owned_value_path!("prCount"),
    update_count: owned_value_path!("upCount"),
    ad_count: owned_value_path!("adCount"),
    extended_rcode: owned_value_path!("extendedRcode"),
    version: owned_value_path!("ednsVersion"),
    do_flag: owned_value_path!("do"),
    udp_max_payload_size: owned_value_path!("udpPayloadSize"),
    ede: owned_value_path!("ede"),
    options: owned_value_path!("options"),
    info_code: owned_value_path!("infoCode"),
    purpose: owned_value_path!("purpose"),
    extra_text: owned_value_path!("extraText"),
    opt_code: owned_value_path!("optCode"),
    opt_name: owned_value_path!("optName"),
    opt_data: owned_value_path!("optValue"),
    record_type: owned_value_path!("recordType"),
    record_type_id: owned_value_path!("recordTypeId"),
    ttl: owned_value_path!("ttl"),
    class: owned_value_path!("class"),
    rdata: owned_value_path!("rData"),
    rdata_bytes: owned_value_path!("rDataBytes"),
    domain_name: owned_value_path!("domainName"),
    question_type: owned_value_path!("questionType"),
    question_type_id: owned_value_path!("questionTypeId"),
    zone_name: owned_value_path!("zName"),
    zone_class: owned_value_path!("zClass"),
    zone_type: owned_value_path!("zType"),
    zone_type_id: owned_value_path!("zTypeId"),
});

#[derive(Debug, Default, Clone)]
pub struct DnsQueryHeaderSchema;

impl DnsQueryHeaderSchema {
    pub fn schema_definition() -> Collection<Field> {
        btreemap! {
            DNSTAP_VALUE_PATHS.id.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.opcode.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.rcode.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.qr.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.aa.to_string() => Kind::boolean(),
            DNSTAP_VALUE_PATHS.tc.to_string() => Kind::boolean(),
            DNSTAP_VALUE_PATHS.rd.to_string() => Kind::boolean(),
            DNSTAP_VALUE_PATHS.ra.to_string() => Kind::boolean(),
            DNSTAP_VALUE_PATHS.ad.to_string() => Kind::boolean(),
            DNSTAP_VALUE_PATHS.cd.to_string() => Kind::boolean(),
            DNSTAP_VALUE_PATHS.ar_count.to_string() => Kind::integer().or_undefined(),
            DNSTAP_VALUE_PATHS.question_count.to_string() => Kind::integer().or_undefined(),
            DNSTAP_VALUE_PATHS.answer_count.to_string() => Kind::integer().or_undefined(),
            DNSTAP_VALUE_PATHS.authority_count.to_string() => Kind::integer().or_undefined(),
        }
        .into()
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsUpdateHeaderSchema;

impl DnsUpdateHeaderSchema {
    pub fn schema_definition() -> Collection<Field> {
        btreemap! {
            DNSTAP_VALUE_PATHS.id.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.opcode.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.rcode.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.qr.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.zone_count.to_string() => Kind::integer().or_undefined(),
            DNSTAP_VALUE_PATHS.prerequisite_count.to_string() => Kind::integer().or_undefined(),
            DNSTAP_VALUE_PATHS.update_count.to_string() => Kind::integer().or_undefined(),
            DNSTAP_VALUE_PATHS.ad_count.to_string() => Kind::integer().or_undefined(),
        }
        .into()
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsMessageOptPseudoSectionSchema;

impl DnsMessageOptPseudoSectionSchema {
    pub fn schema_definition() -> Collection<Field> {
        btreemap! {
            DNSTAP_VALUE_PATHS.extended_rcode.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.version.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.do_flag.to_string() => Kind::boolean(),
            DNSTAP_VALUE_PATHS.udp_max_payload_size.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.options.to_string() => Kind::array(
                Collection::from_unknown(Kind::object(DnsMessageOptionSchema::schema_definition()))
            ).or_undefined(),
        }
        .into()
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsMessageEdeOptionSchema;

impl DnsMessageEdeOptionSchema {
    pub fn schema_definition() -> Collection<Field> {
        btreemap! {
            DNSTAP_VALUE_PATHS.info_code.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.purpose.to_string() => Kind::bytes(),
            DNSTAP_VALUE_PATHS.extra_text.to_string() => Kind::bytes(),
        }
        .into()
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsMessageOptionSchema;

impl DnsMessageOptionSchema {
    pub fn schema_definition() -> Collection<Field> {
        btreemap! {
            DNSTAP_VALUE_PATHS.opt_code.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.opt_name.to_string() => Kind::bytes(),
            DNSTAP_VALUE_PATHS.opt_data.to_string() => Kind::bytes(),
        }
        .into()
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsRecordSchema;

impl DnsRecordSchema {
    pub fn schema_definition() -> Collection<Field> {
        btreemap! {
            DNSTAP_VALUE_PATHS.domain_name.to_string() => Kind::bytes(),
            DNSTAP_VALUE_PATHS.record_type.to_string() => Kind::bytes().or_undefined(),
            DNSTAP_VALUE_PATHS.record_type_id.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.ttl.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.class.to_string() => Kind::bytes(),
            DNSTAP_VALUE_PATHS.rdata.to_string() => Kind::bytes(),
            DNSTAP_VALUE_PATHS.rdata_bytes.to_string() => Kind::bytes().or_undefined(),
        }
        .into()
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsQueryQuestionSchema;

impl DnsQueryQuestionSchema {
    pub fn schema_definition() -> Collection<Field> {
        btreemap! {
            DNSTAP_VALUE_PATHS.class.to_string() => Kind::bytes(),
            DNSTAP_VALUE_PATHS.domain_name.to_string() => Kind::bytes(),
            DNSTAP_VALUE_PATHS.question_type.to_string() => Kind::bytes(),
            DNSTAP_VALUE_PATHS.question_type_id.to_string() => Kind::integer(),
        }
        .into()
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsUpdateZoneInfoSchema;

impl DnsUpdateZoneInfoSchema {
    pub fn schema_definition() -> Collection<Field> {
        btreemap! {
            DNSTAP_VALUE_PATHS.zone_name.to_string() => Kind::bytes(),
            DNSTAP_VALUE_PATHS.zone_type.to_string() => Kind::bytes().or_undefined(),
            DNSTAP_VALUE_PATHS.zone_type_id.to_string() => Kind::integer(),
            DNSTAP_VALUE_PATHS.zone_class.to_string() => Kind::bytes(),
        }
        .into()
    }
}
