use lookup::{owned_value_path, OwnedValuePath};
use std::collections::BTreeMap;
use vrl::value::btreemap;
use vrl::value::{
    kind::{Collection, Field},
    Kind,
};

#[derive(Debug, Default, Clone)]
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
    /// The message schema for the request and response message fields
    fn request_message_schema_definition(&self) -> Collection<Field> {
        let mut result = BTreeMap::new();
        result.insert(
            self.dns_query_message_schema().time().into(),
            Kind::integer().or_undefined(),
        );

        result.insert(
            self.dns_update_message_schema().time().into(),
            Kind::integer().or_undefined(),
        );
        result.insert(
            self.dns_query_message_schema().time_precision().into(),
            Kind::bytes().or_undefined(),
        );
        result.insert(
            self.dns_update_message_schema().time_precision().into(),
            Kind::bytes().or_undefined(),
        );
        result.insert(
            self.dns_query_message_schema().response_code().into(),
            Kind::integer().or_undefined(),
        );
        result.insert(
            self.dns_update_message_schema().response_code().into(),
            Kind::integer().or_undefined(),
        );
        result.insert(
            self.dns_query_message_schema().response().into(),
            Kind::bytes().or_undefined(),
        );
        result.insert(
            self.dns_update_message_schema().response().into(),
            Kind::bytes().or_undefined(),
        );

        if self.dns_query_message_schema().header() == self.dns_update_message_schema().header() {
            // This branch will always be hit -
            // we know that both headers are equal since they both pull the values from the common schema.
            let mut schema = self.dns_query_header_schema().schema_definition();
            schema.merge(self.dns_update_header_schema().schema_definition(), true);

            result.insert(
                self.dns_query_message_schema().header().into(),
                Kind::object(schema).or_undefined(),
            );
        } else {
            result.insert(
                self.dns_query_message_schema().header().into(),
                Kind::object(self.dns_query_header_schema().schema_definition()).or_undefined(),
            );
            result.insert(
                self.dns_update_message_schema().header().into(),
                Kind::object(self.dns_update_header_schema().schema_definition()).or_undefined(),
            );
        }
        result.insert(
            self.dns_update_message_schema().zone_section().into(),
            Kind::object(self.dns_update_zone_info_schema().schema_definition()).or_undefined(),
        );

        result.insert(
            self.dns_query_message_schema().question_section().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                self.dns_query_question_schema().schema_definition(),
            )))
            .or_undefined(),
        );

        result.insert(
            self.dns_query_message_schema().answer_section().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                self.dns_record_schema().schema_definition(),
            )))
            .or_undefined(),
        );

        result.insert(
            self.dns_query_message_schema().authority_section().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                self.dns_record_schema().schema_definition(),
            )))
            .or_undefined(),
        );

        result.insert(
            self.dns_query_message_schema().additional_section().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                self.dns_record_schema().schema_definition(),
            )))
            .or_undefined(),
        );

        result.insert(
            self.dns_query_message_schema().opt_pseudo_section().into(),
            Kind::object(
                self.dns_message_opt_pseudo_section_schema()
                    .schema_definition(self.dns_message_option_schema()),
            )
            .or_undefined(),
        );

        result.insert(
            self.dns_query_message_schema().raw_data().into(),
            Kind::bytes().or_undefined(),
        );

        result.insert(
            self.dns_update_message_schema()
                .prerequisite_section()
                .into(),
            Kind::array(Collection::from_unknown(Kind::object(
                self.dns_record_schema().schema_definition(),
            )))
            .or_undefined(),
        );

        result.insert(
            self.dns_update_message_schema().update_section().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                self.dns_record_schema().schema_definition(),
            )))
            .or_undefined(),
        );

        result.insert(
            self.dns_update_message_schema().additional_section().into(),
            Kind::array(Collection::from_unknown(Kind::object(
                self.dns_record_schema().schema_definition(),
            )))
            .or_undefined(),
        );

        result.into()
    }

    /// Schema definition for fields stored in the root.
    fn root_schema_definition(
        &self,
        schema: vector_core::schema::Definition,
    ) -> vector_core::schema::Definition {
        schema
            .optional_field(
                &owned_value_path!(self.dnstap_root_data_schema().server_identity()),
                Kind::bytes(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_root_data_schema().server_version()),
                Kind::bytes(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_root_data_schema().extra()),
                Kind::bytes(),
                None,
            )
            .with_event_field(
                &owned_value_path!(self.dnstap_root_data_schema().data_type_id()),
                Kind::integer(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_root_data_schema().data_type()),
                Kind::bytes(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_root_data_schema().error()),
                Kind::bytes(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_root_data_schema().raw_data()),
                Kind::bytes(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_root_data_schema().time()),
                Kind::integer(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_root_data_schema().time_precision()),
                Kind::bytes(),
                None,
            )
    }

    /// Schema definition from the message.
    pub fn message_schema_definition(
        &self,
        schema: vector_core::schema::Definition,
    ) -> vector_core::schema::Definition {
        schema
            .optional_field(
                &owned_value_path!(self.dnstap_message_schema().socket_family()),
                Kind::bytes(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_message_schema().socket_protocol()),
                Kind::bytes(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_message_schema().query_address()),
                Kind::bytes(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_message_schema().query_port()),
                Kind::integer(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_message_schema().response_address()),
                Kind::bytes(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_message_schema().response_port()),
                Kind::integer(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_message_schema().query_zone()),
                Kind::bytes(),
                None,
            )
            .with_event_field(
                &owned_value_path!(self.dnstap_message_schema().dnstap_message_type_id()),
                Kind::integer(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_message_schema().dnstap_message_type()),
                Kind::bytes(),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_message_schema().request_message()),
                Kind::object(self.request_message_schema_definition()),
                None,
            )
            .optional_field(
                &owned_value_path!(self.dnstap_message_schema().response_message()),
                Kind::object(self.request_message_schema_definition()),
                None,
            )
    }

    /// The schema definition for a dns tap message.
    pub fn schema_definition(
        &self,
        schema: vector_core::schema::Definition,
    ) -> vector_core::schema::Definition {
        self.root_schema_definition(self.message_schema_definition(schema))
    }

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
        Self::default()
    }
}

#[derive(Debug, Clone)]
pub struct DnstapRootDataSchema {
    timestamp: Option<OwnedValuePath>,
}

impl Default for DnstapRootDataSchema {
    fn default() -> Self {
        Self {
            timestamp: Some(owned_value_path!("timestamp")),
        }
    }
}

impl DnstapRootDataSchema {
    pub const fn server_identity(&self) -> &'static str {
        "serverId"
    }

    pub const fn server_version(&self) -> &'static str {
        "serverVersion"
    }

    pub const fn extra(&self) -> &'static str {
        "extraInfo"
    }

    pub const fn data_type(&self) -> &'static str {
        "dataType"
    }

    pub const fn data_type_id(&self) -> &'static str {
        "dataTypeId"
    }

    pub const fn timestamp(&self) -> Option<&OwnedValuePath> {
        self.timestamp.as_ref()
    }

    pub const fn time(&self) -> &'static str {
        "time"
    }

    pub const fn time_precision(&self) -> &'static str {
        "timePrecision"
    }

    pub const fn error(&self) -> &'static str {
        "error"
    }

    pub const fn raw_data(&self) -> &'static str {
        "rawData"
    }

    pub fn set_timestamp(&mut self, val: Option<OwnedValuePath>) -> &mut Self {
        self.timestamp = val;
        self
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnstapMessageSchema;

impl DnstapMessageSchema {
    pub const fn socket_family(&self) -> &'static str {
        "socketFamily"
    }

    pub const fn socket_protocol(&self) -> &'static str {
        "socketProtocol"
    }

    pub const fn query_address(&self) -> &'static str {
        "sourceAddress"
    }

    pub const fn query_port(&self) -> &'static str {
        "sourcePort"
    }

    pub const fn response_address(&self) -> &'static str {
        "responseAddress"
    }

    pub const fn response_port(&self) -> &'static str {
        "responsePort"
    }

    pub const fn query_zone(&self) -> &'static str {
        "queryZone"
    }

    pub const fn dnstap_message_type(&self) -> &'static str {
        "messageType"
    }

    pub const fn dnstap_message_type_id(&self) -> &'static str {
        "messageTypeId"
    }

    pub const fn request_message(&self) -> &'static str {
        "requestData"
    }

    pub const fn response_message(&self) -> &'static str {
        "responseData"
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsMessageCommonSchema;

impl DnsMessageCommonSchema {
    pub const fn response_code() -> &'static str {
        "fullRcode"
    }

    pub const fn response() -> &'static str {
        "rcodeName"
    }

    pub const fn time() -> &'static str {
        "time"
    }

    pub const fn time_precision() -> &'static str {
        "timePrecision"
    }

    pub const fn raw_data() -> &'static str {
        "rawData"
    }

    pub const fn header() -> &'static str {
        "header"
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsQueryMessageSchema;

impl DnsQueryMessageSchema {
    pub const fn response_code(&self) -> &'static str {
        DnsMessageCommonSchema::response_code()
    }

    pub const fn response(&self) -> &'static str {
        DnsMessageCommonSchema::response()
    }

    pub const fn time(&self) -> &'static str {
        DnsMessageCommonSchema::time()
    }

    pub const fn time_precision(&self) -> &'static str {
        DnsMessageCommonSchema::time_precision()
    }

    pub const fn raw_data(&self) -> &'static str {
        DnsMessageCommonSchema::raw_data()
    }

    pub const fn header(&self) -> &'static str {
        DnsMessageCommonSchema::header()
    }

    pub const fn question_section(&self) -> &'static str {
        "question"
    }

    pub const fn answer_section(&self) -> &'static str {
        "answers"
    }

    pub const fn authority_section(&self) -> &'static str {
        "authority"
    }

    pub const fn additional_section(&self) -> &'static str {
        "additional"
    }

    pub const fn opt_pseudo_section(&self) -> &'static str {
        "opt"
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsUpdateMessageSchema;

impl DnsUpdateMessageSchema {
    pub const fn response_code(&self) -> &'static str {
        DnsMessageCommonSchema::response_code()
    }

    pub const fn response(&self) -> &'static str {
        DnsMessageCommonSchema::response()
    }

    pub const fn time(&self) -> &'static str {
        DnsMessageCommonSchema::time()
    }

    pub const fn time_precision(&self) -> &'static str {
        DnsMessageCommonSchema::time_precision()
    }

    pub const fn raw_data(&self) -> &'static str {
        DnsMessageCommonSchema::raw_data()
    }

    pub const fn header(&self) -> &'static str {
        DnsMessageCommonSchema::header()
    }

    pub const fn zone_section(&self) -> &'static str {
        "zone"
    }

    pub const fn prerequisite_section(&self) -> &'static str {
        "prerequisite"
    }

    pub const fn update_section(&self) -> &'static str {
        "update"
    }

    pub const fn additional_section(&self) -> &'static str {
        "additional"
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsMessageHeaderCommonSchema;

impl DnsMessageHeaderCommonSchema {
    pub const fn id() -> &'static str {
        "id"
    }

    pub const fn opcode() -> &'static str {
        "opcode"
    }

    pub const fn rcode() -> &'static str {
        "rcode"
    }

    pub const fn qr() -> &'static str {
        "qr"
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsQueryHeaderSchema;

impl DnsQueryHeaderSchema {
    pub fn schema_definition(&self) -> Collection<Field> {
        btreemap! {
            self.id() => Kind::integer(),
            self.opcode() => Kind::integer(),
            self.rcode() => Kind::integer(),
            self.qr() => Kind::integer(),
            self.aa() => Kind::boolean(),
            self.tc() => Kind::boolean(),
            self.rd() => Kind::boolean(),
            self.ra() => Kind::boolean(),
            self.ad() => Kind::boolean(),
            self.cd() => Kind::boolean(),
            self.additional_count() => Kind::integer().or_undefined(),
            self.question_count() => Kind::integer().or_undefined(),
            self.answer_count() => Kind::integer().or_undefined(),
            self.authority_count() => Kind::integer().or_undefined(),
        }
        .into()
    }

    pub const fn id(&self) -> &'static str {
        DnsMessageHeaderCommonSchema::id()
    }

    pub const fn opcode(&self) -> &'static str {
        DnsMessageHeaderCommonSchema::opcode()
    }

    pub const fn rcode(&self) -> &'static str {
        DnsMessageHeaderCommonSchema::rcode()
    }

    pub const fn qr(&self) -> &'static str {
        DnsMessageHeaderCommonSchema::qr()
    }

    pub const fn aa(&self) -> &'static str {
        "aa"
    }

    pub const fn tc(&self) -> &'static str {
        "tc"
    }

    pub const fn rd(&self) -> &'static str {
        "rd"
    }

    pub const fn ra(&self) -> &'static str {
        "ra"
    }

    pub const fn ad(&self) -> &'static str {
        "ad"
    }

    pub const fn cd(&self) -> &'static str {
        "cd"
    }

    pub const fn question_count(&self) -> &'static str {
        "qdCount"
    }

    pub const fn answer_count(&self) -> &'static str {
        "anCount"
    }

    pub const fn authority_count(&self) -> &'static str {
        "nsCount"
    }

    pub const fn additional_count(&self) -> &'static str {
        "arCount"
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsUpdateHeaderSchema;

impl DnsUpdateHeaderSchema {
    pub fn schema_definition(&self) -> Collection<Field> {
        btreemap! {
            self.id() => Kind::integer(),
            self.opcode() => Kind::integer(),
            self.rcode() => Kind::integer(),
            self.qr() => Kind::integer(),
            self.zone_count() => Kind::integer().or_undefined(),
            self.prerequisite_count() => Kind::integer().or_undefined(),
            self.update_count() => Kind::integer().or_undefined(),
            self.additional_count() => Kind::integer().or_undefined(),
        }
        .into()
    }

    pub const fn id(&self) -> &'static str {
        DnsMessageHeaderCommonSchema::id()
    }

    pub const fn opcode(&self) -> &'static str {
        DnsMessageHeaderCommonSchema::opcode()
    }

    pub const fn rcode(&self) -> &'static str {
        DnsMessageHeaderCommonSchema::rcode()
    }

    pub const fn qr(&self) -> &'static str {
        DnsMessageHeaderCommonSchema::qr()
    }

    pub const fn zone_count(&self) -> &'static str {
        "zoCount"
    }

    pub const fn prerequisite_count(&self) -> &'static str {
        "prCount"
    }

    pub const fn update_count(&self) -> &'static str {
        "upCount"
    }

    pub const fn additional_count(&self) -> &'static str {
        "adCount"
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsMessageOptPseudoSectionSchema;

impl DnsMessageOptPseudoSectionSchema {
    pub fn schema_definition(
        &self,
        dns_message_option_schema: &DnsMessageOptionSchema,
    ) -> Collection<Field> {
        btreemap! {
            self.extended_rcode() => Kind::integer(),
            self.version() => Kind::integer(),
            self.do_flag() => Kind::boolean(),
            self.udp_max_payload_size() => Kind::integer(),
            self.options() => Kind::array(
                Collection::from_unknown(Kind::object(dns_message_option_schema.schema_definition()))
            ).or_undefined(),
        }
        .into()
    }

    pub const fn extended_rcode(&self) -> &'static str {
        "extendedRcode"
    }

    pub const fn version(&self) -> &'static str {
        "ednsVersion"
    }

    pub const fn do_flag(&self) -> &'static str {
        "do"
    }

    pub const fn udp_max_payload_size(&self) -> &'static str {
        "udpPayloadSize"
    }

    pub const fn options(&self) -> &'static str {
        "options"
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsMessageOptionSchema;

impl DnsMessageOptionSchema {
    pub fn schema_definition(&self) -> Collection<Field> {
        btreemap! {
            self.opt_code() => Kind::integer(),
            self.opt_name() => Kind::bytes(),
            self.opt_data() => Kind::bytes(),
        }
        .into()
    }

    pub const fn opt_code(&self) -> &'static str {
        "optCode"
    }

    pub const fn opt_name(&self) -> &'static str {
        "optName"
    }

    pub const fn opt_data(&self) -> &'static str {
        "optValue"
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsRecordSchema;

impl DnsRecordSchema {
    pub fn schema_definition(&self) -> Collection<Field> {
        btreemap! {
            self.name() => Kind::bytes(),
            self.record_type() => Kind::bytes().or_undefined(),
            self.record_type_id() => Kind::integer(),
            self.ttl() => Kind::integer(),
            self.class() => Kind::bytes(),
            self.rdata() => Kind::bytes(),
            self.rdata_bytes() => Kind::bytes().or_undefined(),
        }
        .into()
    }

    pub const fn name(&self) -> &'static str {
        "domainName"
    }

    pub const fn record_type(&self) -> &'static str {
        "recordType"
    }

    pub const fn record_type_id(&self) -> &'static str {
        "recordTypeId"
    }

    pub const fn ttl(&self) -> &'static str {
        "ttl"
    }

    pub const fn class(&self) -> &'static str {
        "class"
    }

    pub const fn rdata(&self) -> &'static str {
        "rData"
    }

    pub const fn rdata_bytes(&self) -> &'static str {
        "rDataBytes"
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsQueryQuestionSchema;

impl DnsQueryQuestionSchema {
    pub fn schema_definition(&self) -> Collection<Field> {
        btreemap! {
            self.class() => Kind::bytes(),
            self.name() => Kind::bytes(),
            self.question_type() => Kind::bytes(),
            self.question_type_id() => Kind::integer(),
        }
        .into()
    }

    pub const fn name(&self) -> &'static str {
        "domainName"
    }

    pub const fn question_type(&self) -> &'static str {
        "questionType"
    }

    pub const fn question_type_id(&self) -> &'static str {
        "questionTypeId"
    }

    pub const fn class(&self) -> &'static str {
        "class"
    }
}

#[derive(Debug, Default, Clone)]
pub struct DnsUpdateZoneInfoSchema;

impl DnsUpdateZoneInfoSchema {
    pub fn schema_definition(&self) -> Collection<Field> {
        btreemap! {
            self.zone_name() => Kind::bytes(),
            self.zone_type() => Kind::bytes().or_undefined(),
            self.zone_type_id() => Kind::integer(),
            self.zone_class() => Kind::bytes(),
        }
        .into()
    }

    pub const fn zone_name(&self) -> &'static str {
        "zName"
    }

    pub const fn zone_class(&self) -> &'static str {
        "zClass"
    }

    pub const fn zone_type(&self) -> &'static str {
        "zType"
    }

    pub const fn zone_type_id(&self) -> &'static str {
        "zTypeId"
    }
}
