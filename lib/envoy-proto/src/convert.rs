use super::{
    envoy::config::core::v3::{
        address, envoy_internal_address::AddressNameSpecifier, node::UserAgentVersionType,
        socket_address::PortSpecifier, socket_address::Protocol, Address, BuildVersion,
        EnvoyInternalAddress, Extension, Locality, Metadata, Node, Pipe, RequestMethod,
        SocketAddress,
    },
    envoy::data::accesslog::v3::{
        http_access_log_entry::HttpVersion, response_flags, response_flags::unauthorized,
        tls_properties, tls_properties::certificate_properties,
        tls_properties::certificate_properties::subject_alt_name, AccessLogCommon,
        HttpAccessLogEntry, HttpRequestProperties, HttpResponseProperties, ResponseFlags,
        TlsProperties,
    },
    envoy::r#type::v3::SemanticVersion,
    envoy::service::accesslog::v3::stream_access_logs_message::Identifier,
    xds::core::v3::ContextParams,
};
use chrono::{TimeZone, Utc};
use ordered_float::NotNan;
use prost_types::Struct;
use std::collections::BTreeMap;
use value::Value;

const SEC_IN_NANOS: i64 = 1_000_000_000;

const ADDRESS_KEY: &str = "address";
const ADDRESS_NAME_SPECIFIER_KEY: &str = "address_name_specifier";
const AUTHORITY_KEY: &str = "authority";
const BUILD_VERSION_KEY: &str = "version";
const CATEGORY_KEY: &str = "category";
const CLIENT_FEATURES_KEY: &str = "client_features";
const CLUSTER_KEY: &str = "cluster";
const COMMON_PROPERTIES_KEY: &str = "common_properties";
const CONNECTION_TERMINATION_DETAILS_KEY: &str = "connection_termination_details";
const CUSTOM_TAGS_KEY: &str = "custom_tags";
const DELAY_INJECTED_KEY: &str = "delay_injected";
const DISABLED_KEY: &str = "disabled";
const DNS_KEY: &str = "dns";
const DNS_RESOLUTION_FAILURE_KEY: &str = "dns_resolution_failure";
const DOWNSTREAM_CONNECTION_TERMINATION_KEY: &str = "downstream_connection_termination";
const DOWNSTREAM_DIRECT_REMOTE_ADDRESS_KEY: &str = "downstream_direct_remote_address";
const DOWNSTREAM_LOCAL_ADDRESS_KEY: &str = "downstream_local_address";
const DOWNSTREAM_PROTOCOL_ERROR_KEY: &str = "downstream_protocol_error";
const DOWNSTREAM_REMOTE_ADDRESS_KEY: &str = "downstream_remote_address";
const DURATION_KEY: &str = "duration_ns";
const DURATION_TIMEOUT_KEY: &str = "duration_timeout";
const DYNAMIC_PARAMETERS_KEY: &str = "dynamic_parameters";
const ENDPOINT_ID_KEY: &str = "endpoint_id";
const ENVOY_INTERNAL_ADDRESS_KEY: &str = "envoy_internal_address";
const EXTENSIONS_KEY: &str = "extensions";
const FAILED_LOCAL_HEALTHCHECK_KEY: &str = "failed_local_healthcheck";
const FAULT_INJECTED_KEY: &str = "fault_injected";
const FILTER_METADATA_KEY: &str = "filter_metadata";
const FILTER_STATE_OBJECTS_KEY: &str = "filter_state_objects";
const FORWARDED_FOR_KEY: &str = "forwarded_for";
const ID_KEY: &str = "id";
const INVALID_ENVOY_REQUEST_HEADERS_KEY: &str = "invalid_envoy_request_headers";
const IPV4_COMPAT_KEY: &str = "ipv4_compat";
const JA3_FINGERPRINT_KEY: &str = "ja3_fingerprint";
const LOCALITY_KEY: &str = "locality";
const LOCAL_CERTIFICATE_PROPERTIES_KEY: &str = "local_certificate_properties";
const LOCAL_RESET_KEY: &str = "local_reset";
const LOG_NAME_KEY: &str = "log_name";
const MAJOR_NUM_KEY: &str = "major_number";
const METADATA_KEY: &str = "metadata";
const MINOR_NUM_KEY: &str = "minor_number";
const MODE_KEY: &str = "mode";
const NAMED_PORT_KEY: &str = "named_port";
const NAME_KEY: &str = "name";
const NODE_KEY: &str = "node";
const NO_CLUSTER_FOUND_KEY: &str = "no_cluster_found";
const NO_FILTER_CONFIG_FOUND_KEY: &str = "no_filter_config_found";
const NO_HEALTHY_UPSTREAM_KEY: &str = "no_healthy_upstream";
const NO_ROUTE_FOUND_KEY: &str = "no_route_found";
const ORIGINAL_PATH_KEY: &str = "original_path";
const OVERLOAD_MANAGER_KEY: &str = "overload_manager";
const PARAMS_KEY: &str = "params";
const PATCH_NUM_KEY: &str = "patch";
const PATH_KEY: &str = "path";
const PEER_CERTIFICATE_PROPERTIES_KEY: &str = "peer_certificate_properties";
const PIPE_KEY: &str = "pipe";
const PORT_KEY: &str = "port";
const PORT_SPECIFIER_KEY: &str = "port_specifier";
const PORT_VALUE_KEY: &str = "port_value";
const PROTOCOL_KEY: &str = "protocol";
const PROTOCOL_VERSION_KEY: &str = "protocol_version";
const RATE_LIMITED_KEY: &str = "rate_limited";
const RATE_LIMIT_SERVICE_ERROR_KEY: &str = "rate_limit_service_error";
const REASON_KEY: &str = "reason";
const REFERER_KEY: &str = "referer";
const REGION_KEY: &str = "region";
const REQUEST_BODY_BYTES_KEY: &str = "request_body_bytes";
const REQUEST_HEADERS_BYTES_KEY: &str = "request_headers_bytes";
const REQUEST_HEADERS_KEY: &str = "request_headers";
const REQUEST_ID_KEY: &str = "request_id";
const REQUEST_KEY: &str = "request";
const REQUEST_METHOD_KEY: &str = "request_method";
const RESOLVER_NAME_KEY: &str = "resolver_name";
const RESPONSE_BODY_BYTES_KEY: &str = "response_body_bytes";
const RESPONSE_CODE_DETAILS_KEY: &str = "response_code_details";
const RESPONSE_CODE_KEY: &str = "response_code";
const RESPONSE_FLAGS_KEY: &str = "response_flags";
const RESPONSE_FROM_CACHE_FILTER_KEY: &str = "response_from_cache_filter";
const RESPONSE_HEADERS_BYTES_KEY: &str = "response_headers_bytes";
const RESPONSE_HEADERS_KEY: &str = "response_headers";
const RESPONSE_KEY: &str = "response";
const RESPONSE_TRAILERS_KEY: &str = "response_trailers";
const ROUTE_NAME_KEY: &str = "route_name";
const SAN_KEY: &str = "san";
const SCHEME_KEY: &str = "scheme";
const SEM_VER_KEY: &str = "version";
const SERVER_LISTENER_NAME_KEY: &str = "server_listener_name";
const SOCKET_ADDRESS_KEY: &str = "socket_address";
const START_TIME_KEY: &str = "start_time";
const STREAM_IDLE_TIMEOUT_KEY: &str = "stream_idle_timeout";
const SUBJECT_ALT_NAME_KEY: &str = "subject_alt_name";
const SUBJECT_KEY: &str = "subject";
const SUB_ZONE_KEY: &str = "sub_zone";
const TIME_TO_FIRST_DOWNSTREAM_TX_BYTE_KEY: &str = "time_to_first_downstream_tx_byte_ns";
const TIME_TO_FIRST_UPSTREAM_RX_BYTE_KEY: &str = "time_to_first_upstream_rx_byte_ns";
const TIME_TO_FIRST_UPSTREAM_TX_BYTE_KEY: &str = "time_to_first_upstream_tx_byte_ns";
const TIME_TO_LAST_DOWNSTREAM_TX_BYTE_KEY: &str = "time_to_last_downstream_tx_byte_ns";
const TIME_TO_LAST_RX_BYTE_KEY: &str = "time_to_last_rx_byte_ns";
const TIME_TO_LAST_UPSTREAM_RX_BYTE_KEY: &str = "time_to_last_upstream_rx_byte_ns";
const TIME_TO_LAST_UPSTREAM_TX_BYTE_KEY: &str = "time_to_last_upstream_tx_byte_ns";
const TLS_CIPHER_SUITE_KEY: &str = "tls_cipher_suite";
const TLS_PROPERTIES_KEY: &str = "tls_properties";
const TLS_SESSION_ID_KEY: &str = "tls_session_id";
const TLS_SNI_HOSTNAME_KEY: &str = "tls_sni_hostname";
const TLS_VERSION_KEY: &str = "tls_version";
const TYPED_FILTER_METADATA_KEY: &str = "typed_filter_metadata";
const TYPE_URLS_KEY: &str = "type_urls";
const TYPE_URL_KEY: &str = "type_url";
const UNAUTHORIZED_DETAILS_KEY: &str = "unauthorized_details";
const UPSTREAM_CLUSTER_KEY: &str = "upstream_cluster";
const UPSTREAM_CONNECTION_FAILURE_KEY: &str = "upstream_connection_failure";
const UPSTREAM_CONNECTION_TERMINATION_KEY: &str = "upstream_connection_termination";
const UPSTREAM_LOCAL_ADDRESS_KEY: &str = "upstream_local_address";
const UPSTREAM_MAX_STREAM_DURATION_REACHED_KEY: &str = "upstream_max_stream_duration_reached";
const UPSTREAM_OVERFLOW_KEY: &str = "upstream_overflow";
const UPSTREAM_PROTOCOL_ERROR_KEY: &str = "upstream_protocol_error";
const UPSTREAM_REMOTE_ADDRESS_KEY: &str = "upstream_remote_address";
const UPSTREAM_REMOTE_RESET_KEY: &str = "upstream_remote_reset";
const UPSTREAM_REQUEST_ATTEMPT_COUNT_KEY: &str = "upstream_request_attempt_count";
const UPSTREAM_REQUEST_TIMEOUT_KEY: &str = "upstream_request_timeout";
const UPSTREAM_RETRY_LIMIT_EXCEEDED_KEY: &str = "upstream_retry_limit_exceeded";
const UPSTREAM_TRANSPORT_FAILURE_REASON_KEY: &str = "upstream_transport_failure_reason";
const URI_KEY: &str = "uri";
const USER_AGENT_KEY: &str = "user_agent";
const USER_AGENT_NAME_KEY: &str = "user_agent_name";
const USER_AGENT_VERSION_KEY: &str = "user_agent_version";
const USER_AGENT_VERSION_TYPE_KEY: &str = "user_agent_version_type";
const VALUE_KEY: &str = "value";
const ZONE_KEY: &str = "zone";

impl From<Identifier> for Value {
    fn from(identifier: Identifier) -> Self {
        let mut id_map = BTreeMap::new();
        id_map.insert(
            String::from(LOG_NAME_KEY),
            Value::Bytes(identifier.log_name.into()),
        );
        if let Some(node) = identifier.node {
            id_map.insert(String::from(NODE_KEY), Value::from(node));
        }
        Value::Object(id_map)
    }
}

impl From<Node> for Value {
    fn from(node: Node) -> Self {
        let mut node_map = BTreeMap::new();
        node_map.insert(String::from(ID_KEY), Value::Bytes(node.id.into()));
        node_map.insert(String::from(CLUSTER_KEY), Value::Bytes(node.cluster.into()));
        if let Some(metadata) = node.metadata {
            node_map.insert(String::from(METADATA_KEY), prost_struct_to_value(metadata));
        }
        let mut dynamic_params_map = BTreeMap::new();
        for (key, val) in node.dynamic_parameters {
            dynamic_params_map.insert(key, Value::from(val));
        }
        node_map.insert(
            String::from(DYNAMIC_PARAMETERS_KEY),
            Value::Object(dynamic_params_map),
        );
        if let Some(locality) = node.locality {
            node_map.insert(String::from(LOCALITY_KEY), Value::from(locality));
        }
        node_map.insert(
            String::from(USER_AGENT_NAME_KEY),
            Value::Bytes(node.user_agent_name.into()),
        );
        node_map.insert(
            String::from(EXTENSIONS_KEY),
            Value::Array(
                node.extensions
                    .iter()
                    .map(|ext| Value::from(ext.clone()))
                    .collect(),
            ),
        );
        node_map.insert(
            String::from(CLIENT_FEATURES_KEY),
            Value::Array(
                node.client_features
                    .iter()
                    .map(|val| Value::Bytes(val.clone().into()))
                    .collect(),
            ),
        );
        if let Some(user_agent_version_type) = node.user_agent_version_type {
            node_map.insert(
                String::from(USER_AGENT_VERSION_TYPE_KEY),
                Value::from(user_agent_version_type),
            );
        }
        Value::Object(node_map)
    }
}

fn prost_struct_to_value(st: Struct) -> Value {
    let mut struct_map = BTreeMap::new();
    for (field, val) in st.fields {
        struct_map.insert(field, prost_value_to_value(&val));
    }
    Value::Object(struct_map)
}

fn prost_value_to_value(val: &prost_types::Value) -> Value {
    match val.kind.clone() {
        Some(kind) => {
            return prost_kind_to_value(kind);
        }
        None => return Value::Null,
    }
}

fn prost_kind_to_value(kind: prost_types::value::Kind) -> Value {
    match kind {
        prost_types::value::Kind::NullValue(_) => Value::Null,
        prost_types::value::Kind::NumberValue(num) => Value::Float(NotNan::new(num).unwrap()), // TODO: How to best handle this
        prost_types::value::Kind::StringValue(s) => Value::Bytes(s.into()),
        prost_types::value::Kind::BoolValue(b) => Value::Boolean(b),
        prost_types::value::Kind::StructValue(st) => prost_struct_to_value(st),
        prost_types::value::Kind::ListValue(l) => Value::Array(
            l.values
                .iter()
                .map(|val| prost_value_to_value(val))
                .collect(),
        ),
    }
}

impl From<ContextParams> for Value {
    fn from(params: ContextParams) -> Self {
        let mut params_map = BTreeMap::new();
        for (key, value) in params.params {
            params_map.insert(key, Value::Bytes(value.into()));
        }
        let mut context_params_map = BTreeMap::new();
        context_params_map.insert(String::from(PARAMS_KEY), Value::Object(params_map));
        Value::Object(context_params_map)
    }
}

impl From<Locality> for Value {
    fn from(locality: Locality) -> Self {
        let mut locality_map = BTreeMap::new();
        locality_map.insert(
            String::from(REGION_KEY),
            Value::Bytes(locality.region.into()),
        );
        locality_map.insert(String::from(ZONE_KEY), Value::Bytes(locality.zone.into()));
        locality_map.insert(
            String::from(SUB_ZONE_KEY),
            Value::Bytes(locality.sub_zone.into()),
        );
        Value::Object(locality_map)
    }
}

impl From<Extension> for Value {
    fn from(ext: Extension) -> Self {
        let mut ext_map = BTreeMap::new();
        ext_map.insert(String::from(NAME_KEY), Value::Bytes(ext.name.into()));
        ext_map.insert(
            String::from(CATEGORY_KEY),
            Value::Bytes(ext.category.into()),
        );
        if let Some(v) = ext.version {
            ext_map.insert(String::from(BUILD_VERSION_KEY), Value::from(v));
        }
        ext_map.insert(String::from(DISABLED_KEY), Value::Boolean(ext.disabled));
        ext_map.insert(
            String::from(TYPE_URLS_KEY),
            Value::Array(
                ext.type_urls
                    .iter()
                    .map(|val| Value::Bytes(val.clone().into()))
                    .collect(),
            ),
        );
        Value::Object(ext_map)
    }
}

impl From<BuildVersion> for Value {
    fn from(bv: BuildVersion) -> Self {
        let mut build_vers_map = BTreeMap::new();
        if let Some(version) = bv.version {
            build_vers_map.insert(String::from(SEM_VER_KEY), Value::from(version));
        }
        if let Some(metadata) = bv.metadata {
            build_vers_map.insert(String::from(METADATA_KEY), prost_struct_to_value(metadata));
        }
        Value::Object(build_vers_map)
    }
}

impl From<SemanticVersion> for Value {
    fn from(sem_ver: SemanticVersion) -> Self {
        let mut sem_ver_map = BTreeMap::new();
        sem_ver_map.insert(
            String::from(MAJOR_NUM_KEY),
            Value::Integer(sem_ver.major_number.into()),
        );
        sem_ver_map.insert(
            String::from(MINOR_NUM_KEY),
            Value::Integer(sem_ver.minor_number.into()),
        );
        sem_ver_map.insert(
            String::from(PATCH_NUM_KEY),
            Value::Integer(sem_ver.patch.into()),
        );
        Value::Object(sem_ver_map)
    }
}

impl From<UserAgentVersionType> for Value {
    fn from(user_agent_ver_type: UserAgentVersionType) -> Self {
        let mut user_agent_ver_type_map = BTreeMap::new();
        match user_agent_ver_type {
            UserAgentVersionType::UserAgentVersion(agent_ver) => {
                user_agent_ver_type_map.insert(
                    String::from(USER_AGENT_VERSION_KEY),
                    Value::Bytes(agent_ver.into()),
                );
            }
            UserAgentVersionType::UserAgentBuildVersion(build_ver) => {
                user_agent_ver_type_map
                    .insert(String::from(BUILD_VERSION_KEY), Value::from(build_ver));
            }
        }
        Value::Object(user_agent_ver_type_map)
    }
}

impl From<HttpAccessLogEntry> for Value {
    fn from(http_log: HttpAccessLogEntry) -> Self {
        let mut log_map = BTreeMap::new();

        if let Some(common_properties) = http_log.common_properties {
            log_map.insert(
                String::from(COMMON_PROPERTIES_KEY),
                Value::from(common_properties),
            );
        }
        if let Some(http_version) = HttpVersion::from_i32(http_log.protocol_version) {
            log_map.insert(
                String::from(PROTOCOL_VERSION_KEY),
                Value::Bytes(http_version.as_str_name().into()),
            );
        }
        if let Some(request) = http_log.request {
            log_map.insert(String::from(REQUEST_KEY), Value::from(request));
        }
        if let Some(response) = http_log.response {
            log_map.insert(String::from(RESPONSE_KEY), Value::from(response));
        }

        Value::Object(log_map)
    }
}

impl From<AccessLogCommon> for Value {
    fn from(common_props: AccessLogCommon) -> Self {
        let mut common_props_map = BTreeMap::new();
        if let Some(downstream_remote_address) = common_props.downstream_remote_address {
            common_props_map.insert(
                String::from(DOWNSTREAM_REMOTE_ADDRESS_KEY),
                Value::from(downstream_remote_address),
            );
        }
        if let Some(downstream_local_address) = common_props.downstream_local_address {
            common_props_map.insert(
                String::from(DOWNSTREAM_LOCAL_ADDRESS_KEY),
                Value::from(downstream_local_address),
            );
        }
        if let Some(tls_properties) = common_props.tls_properties {
            common_props_map.insert(
                String::from(TLS_PROPERTIES_KEY),
                Value::from(tls_properties),
            );
        }
        if let Some(start_time) = common_props.start_time {
            common_props_map.insert(
                String::from(START_TIME_KEY),
                prost_timestamp_to_value(start_time),
            );
        }
        if let Some(time_to_last_rx_byte) = common_props.time_to_last_rx_byte {
            common_props_map.insert(
                String::from(TIME_TO_LAST_RX_BYTE_KEY),
                prost_duration_to_value(time_to_last_rx_byte),
            );
        }
        if let Some(time_to_first_upstream_tx_byte) = common_props.time_to_first_upstream_tx_byte {
            common_props_map.insert(
                String::from(TIME_TO_FIRST_UPSTREAM_TX_BYTE_KEY),
                prost_duration_to_value(time_to_first_upstream_tx_byte),
            );
        }
        if let Some(time_to_last_upstream_tx_byte) = common_props.time_to_last_upstream_tx_byte {
            common_props_map.insert(
                String::from(TIME_TO_LAST_UPSTREAM_TX_BYTE_KEY),
                prost_duration_to_value(time_to_last_upstream_tx_byte),
            );
        }
        if let Some(time_to_first_upstream_rx_byte) = common_props.time_to_first_upstream_rx_byte {
            common_props_map.insert(
                String::from(TIME_TO_FIRST_UPSTREAM_RX_BYTE_KEY),
                prost_duration_to_value(time_to_first_upstream_rx_byte),
            );
        }
        if let Some(time_to_last_upstream_rx_byte) = common_props.time_to_last_upstream_rx_byte {
            common_props_map.insert(
                String::from(TIME_TO_LAST_UPSTREAM_RX_BYTE_KEY),
                prost_duration_to_value(time_to_last_upstream_rx_byte),
            );
        }
        if let Some(time_to_first_downstream_tx_byte) =
            common_props.time_to_first_downstream_tx_byte
        {
            common_props_map.insert(
                String::from(TIME_TO_FIRST_DOWNSTREAM_TX_BYTE_KEY),
                prost_duration_to_value(time_to_first_downstream_tx_byte),
            );
        }
        if let Some(time_to_last_downstream_tx_byte) = common_props.time_to_last_downstream_tx_byte
        {
            common_props_map.insert(
                String::from(TIME_TO_LAST_DOWNSTREAM_TX_BYTE_KEY),
                prost_duration_to_value(time_to_last_downstream_tx_byte),
            );
        }
        if let Some(upstream_remote_address) = common_props.upstream_remote_address {
            common_props_map.insert(
                String::from(UPSTREAM_REMOTE_ADDRESS_KEY),
                Value::from(upstream_remote_address),
            );
        }
        if let Some(upstream_local_address) = common_props.upstream_local_address {
            common_props_map.insert(
                String::from(UPSTREAM_LOCAL_ADDRESS_KEY),
                Value::from(upstream_local_address),
            );
        }
        common_props_map.insert(
            String::from(UPSTREAM_CLUSTER_KEY),
            Value::Bytes(common_props.upstream_cluster.into()),
        );
        if let Some(response_flags) = common_props.response_flags {
            common_props_map.insert(
                String::from(RESPONSE_FLAGS_KEY),
                Value::from(response_flags),
            );
        }
        if let Some(metadata) = common_props.metadata {
            common_props_map.insert(String::from(METADATA_KEY), Value::from(metadata));
        }
        common_props_map.insert(
            String::from(UPSTREAM_TRANSPORT_FAILURE_REASON_KEY),
            Value::Bytes(common_props.upstream_transport_failure_reason.into()),
        );
        common_props_map.insert(
            String::from(ROUTE_NAME_KEY),
            Value::Bytes(common_props.route_name.into()),
        );
        if let Some(downstream_direct_remote_address) =
            common_props.downstream_direct_remote_address
        {
            common_props_map.insert(
                String::from(DOWNSTREAM_DIRECT_REMOTE_ADDRESS_KEY),
                Value::from(downstream_direct_remote_address),
            );
        }
        let mut filter_state_map = BTreeMap::new();
        for (key, val) in common_props.filter_state_objects {
            filter_state_map.insert(key, prost_any_to_value(val));
        }
        common_props_map.insert(
            String::from(FILTER_STATE_OBJECTS_KEY),
            Value::Object(filter_state_map),
        );
        let mut custom_tags_map = BTreeMap::new();
        for (key, val) in common_props.custom_tags {
            custom_tags_map.insert(key, Value::Bytes(val.into()));
        }
        common_props_map.insert(
            String::from(CUSTOM_TAGS_KEY),
            Value::Object(custom_tags_map),
        );
        if let Some(duration) = common_props.duration {
            common_props_map.insert(
                String::from(DURATION_KEY),
                prost_duration_to_value(duration),
            );
        }
        common_props_map.insert(
            String::from(UPSTREAM_REQUEST_ATTEMPT_COUNT_KEY),
            Value::Integer(common_props.upstream_request_attempt_count.into()),
        );
        common_props_map.insert(
            String::from(CONNECTION_TERMINATION_DETAILS_KEY),
            Value::Bytes(common_props.connection_termination_details.into()),
        );
        Value::Object(common_props_map)
    }
}

impl From<Address> for Value {
    fn from(addr: Address) -> Self {
        if let Some(a) = addr.address {
            return Value::from(a);
        }
        Value::Null
    }
}

impl From<address::Address> for Value {
    fn from(addr: address::Address) -> Self {
        let mut addr_map = BTreeMap::new();
        match addr {
            address::Address::SocketAddress(socket_addr) => {
                addr_map.insert(String::from(SOCKET_ADDRESS_KEY), Value::from(socket_addr));
            }
            address::Address::Pipe(pipe) => {
                addr_map.insert(String::from(PIPE_KEY), Value::from(pipe));
            }
            address::Address::EnvoyInternalAddress(internal_addr) => {
                addr_map.insert(
                    String::from(ENVOY_INTERNAL_ADDRESS_KEY),
                    Value::from(internal_addr),
                );
            }
        }
        Value::Object(addr_map)
    }
}

impl From<TlsProperties> for Value {
    fn from(tls_properties: TlsProperties) -> Self {
        let mut tls_properties_map = BTreeMap::new();
        if let Some(tls_version) = tls_properties::TlsVersion::from_i32(tls_properties.tls_version)
        {
            tls_properties_map.insert(String::from(TLS_VERSION_KEY), Value::from(tls_version));
        }
        if let Some(tls_cipher_suite) = tls_properties.tls_cipher_suite {
            tls_properties_map.insert(
                String::from(TLS_CIPHER_SUITE_KEY),
                Value::from(tls_cipher_suite),
            );
        }
        tls_properties_map.insert(
            String::from(TLS_SNI_HOSTNAME_KEY),
            Value::Bytes(tls_properties.tls_sni_hostname.into()),
        );
        if let Some(local_certificate_properties) = tls_properties.local_certificate_properties {
            tls_properties_map.insert(
                String::from(LOCAL_CERTIFICATE_PROPERTIES_KEY),
                Value::from(local_certificate_properties),
            );
        }
        if let Some(peer_certificate_properties) = tls_properties.peer_certificate_properties {
            tls_properties_map.insert(
                String::from(PEER_CERTIFICATE_PROPERTIES_KEY),
                Value::from(peer_certificate_properties),
            );
        }
        tls_properties_map.insert(
            String::from(TLS_SESSION_ID_KEY),
            Value::Bytes(tls_properties.tls_session_id.into()),
        );
        tls_properties_map.insert(
            String::from(JA3_FINGERPRINT_KEY),
            Value::Bytes(tls_properties.ja3_fingerprint.into()),
        );
        Value::Object(tls_properties_map)
    }
}

impl From<tls_properties::TlsVersion> for Value {
    fn from(tls_version: tls_properties::TlsVersion) -> Self {
        let mut tls_version_map = BTreeMap::new();
        tls_version_map.insert(
            String::from(TLS_VERSION_KEY),
            Value::Bytes(tls_version.as_str_name().into()),
        );
        Value::Object(tls_version_map)
    }
}

impl From<tls_properties::CertificateProperties> for Value {
    fn from(my_field: tls_properties::CertificateProperties) -> Self {
        let mut my_map = BTreeMap::new();
        my_map.insert(
            String::from(SUBJECT_ALT_NAME_KEY),
            Value::Array(
                my_field
                    .subject_alt_name
                    .iter()
                    .map(|val| Value::from(val.clone()))
                    .collect(),
            ),
        );
        my_map.insert(
            String::from(SUBJECT_KEY),
            Value::Bytes(my_field.subject.into()),
        );
        Value::Object(my_map)
    }
}

impl From<certificate_properties::SubjectAltName> for Value {
    fn from(san: certificate_properties::SubjectAltName) -> Self {
        let mut san_map = BTreeMap::new();
        san_map.insert(String::from(SAN_KEY), Value::from(san.san));
        Value::Object(san_map)
    }
}

impl From<subject_alt_name::San> for Value {
    fn from(my_field: subject_alt_name::San) -> Self {
        let mut my_map = BTreeMap::new();
        match my_field {
            subject_alt_name::San::Uri(uri) => {
                my_map.insert(String::from(URI_KEY), Value::Bytes(uri.into()));
            }
            subject_alt_name::San::Dns(dns) => {
                my_map.insert(String::from(DNS_KEY), Value::Bytes(dns.into()));
            }
        }
        Value::Object(my_map)
    }
}

impl From<SocketAddress> for Value {
    fn from(socket_addr: SocketAddress) -> Self {
        let mut socket_addr_map = BTreeMap::new();
        if let Some(protocol) = Protocol::from_i32(socket_addr.protocol) {
            socket_addr_map.insert(
                String::from(PROTOCOL_KEY),
                Value::Bytes(protocol.as_str_name().into()),
            );
        }
        socket_addr_map.insert(
            String::from(ADDRESS_KEY),
            Value::Bytes(socket_addr.address.into()),
        );
        socket_addr_map.insert(
            String::from(RESOLVER_NAME_KEY),
            Value::Bytes(socket_addr.resolver_name.into()),
        );
        socket_addr_map.insert(
            String::from(IPV4_COMPAT_KEY),
            Value::Boolean(socket_addr.ipv4_compat),
        );
        if let Some(port_specifier) = socket_addr.port_specifier {
            socket_addr_map.insert(
                String::from(PORT_SPECIFIER_KEY),
                Value::from(port_specifier),
            );
        }
        Value::Object(socket_addr_map)
    }
}

impl From<PortSpecifier> for Value {
    fn from(port_specifier: PortSpecifier) -> Self {
        let mut port_specifier_map = BTreeMap::new();
        match port_specifier {
            PortSpecifier::PortValue(port) => {
                port_specifier_map
                    .insert(String::from(PORT_VALUE_KEY), Value::Integer(port.into()));
            }
            PortSpecifier::NamedPort(named_port) => {
                port_specifier_map.insert(
                    String::from(NAMED_PORT_KEY),
                    Value::Bytes(named_port.into()),
                );
            }
        }
        Value::Object(port_specifier_map)
    }
}

impl From<Pipe> for Value {
    fn from(pipe: Pipe) -> Self {
        let mut pipe_map = BTreeMap::new();
        pipe_map.insert(String::from(PATH_KEY), Value::Bytes(pipe.path.into()));
        pipe_map.insert(String::from(MODE_KEY), Value::Integer(pipe.mode.into()));
        Value::Object(pipe_map)
    }
}

impl From<EnvoyInternalAddress> for Value {
    fn from(internal_addr: EnvoyInternalAddress) -> Self {
        let mut internal_addr_map = BTreeMap::new();
        internal_addr_map.insert(
            String::from(ENDPOINT_ID_KEY),
            Value::Bytes(internal_addr.endpoint_id.into()),
        );
        if let Some(addr_name_specifier) = internal_addr.address_name_specifier {
            internal_addr_map.insert(
                String::from(ADDRESS_NAME_SPECIFIER_KEY),
                Value::from(addr_name_specifier),
            );
        }
        Value::Object(internal_addr_map)
    }
}

impl From<AddressNameSpecifier> for Value {
    fn from(addr_name_specifier: AddressNameSpecifier) -> Self {
        let mut addr_name_specifier_map = BTreeMap::new();
        match addr_name_specifier {
            AddressNameSpecifier::ServerListenerName(name) => {
                addr_name_specifier_map.insert(
                    String::from(SERVER_LISTENER_NAME_KEY),
                    Value::Bytes(name.into()),
                );
            }
        }
        Value::Object(addr_name_specifier_map)
    }
}

fn prost_timestamp_to_value(d: prost_types::Timestamp) -> Value {
    Value::Timestamp(Utc.timestamp(d.seconds, d.nanos as u32))
}

fn prost_duration_to_value(d: prost_types::Duration) -> Value {
    Value::Integer(d.seconds * SEC_IN_NANOS + d.nanos as i64)
}

impl From<ResponseFlags> for Value {
    fn from(response_flags: ResponseFlags) -> Self {
        let mut response_flags_map = BTreeMap::new();
        response_flags_map.insert(
            String::from(FAILED_LOCAL_HEALTHCHECK_KEY),
            Value::Boolean(response_flags.failed_local_healthcheck),
        );
        response_flags_map.insert(
            String::from(NO_HEALTHY_UPSTREAM_KEY),
            Value::Boolean(response_flags.no_healthy_upstream),
        );
        response_flags_map.insert(
            String::from(UPSTREAM_REQUEST_TIMEOUT_KEY),
            Value::Boolean(response_flags.upstream_request_timeout),
        );
        response_flags_map.insert(
            String::from(LOCAL_RESET_KEY),
            Value::Boolean(response_flags.local_reset),
        );
        response_flags_map.insert(
            String::from(UPSTREAM_REMOTE_RESET_KEY),
            Value::Boolean(response_flags.upstream_remote_reset),
        );
        response_flags_map.insert(
            String::from(UPSTREAM_CONNECTION_FAILURE_KEY),
            Value::Boolean(response_flags.upstream_connection_failure),
        );
        response_flags_map.insert(
            String::from(UPSTREAM_CONNECTION_TERMINATION_KEY),
            Value::Boolean(response_flags.upstream_connection_termination),
        );
        response_flags_map.insert(
            String::from(UPSTREAM_OVERFLOW_KEY),
            Value::Boolean(response_flags.upstream_overflow),
        );
        response_flags_map.insert(
            String::from(NO_ROUTE_FOUND_KEY),
            Value::Boolean(response_flags.no_route_found),
        );
        response_flags_map.insert(
            String::from(DELAY_INJECTED_KEY),
            Value::Boolean(response_flags.delay_injected),
        );
        response_flags_map.insert(
            String::from(FAULT_INJECTED_KEY),
            Value::Boolean(response_flags.fault_injected),
        );
        response_flags_map.insert(
            String::from(RATE_LIMITED_KEY),
            Value::Boolean(response_flags.rate_limited),
        );
        if let Some(unauthorized_details) = response_flags.unauthorized_details {
            response_flags_map.insert(
                String::from(UNAUTHORIZED_DETAILS_KEY),
                Value::from(unauthorized_details),
            );
        }
        response_flags_map.insert(
            String::from(RATE_LIMIT_SERVICE_ERROR_KEY),
            Value::Boolean(response_flags.rate_limit_service_error),
        );
        response_flags_map.insert(
            String::from(DOWNSTREAM_CONNECTION_TERMINATION_KEY),
            Value::Boolean(response_flags.downstream_connection_termination),
        );
        response_flags_map.insert(
            String::from(UPSTREAM_RETRY_LIMIT_EXCEEDED_KEY),
            Value::Boolean(response_flags.upstream_retry_limit_exceeded),
        );
        response_flags_map.insert(
            String::from(STREAM_IDLE_TIMEOUT_KEY),
            Value::Boolean(response_flags.stream_idle_timeout),
        );
        response_flags_map.insert(
            String::from(INVALID_ENVOY_REQUEST_HEADERS_KEY),
            Value::Boolean(response_flags.invalid_envoy_request_headers),
        );
        response_flags_map.insert(
            String::from(DOWNSTREAM_PROTOCOL_ERROR_KEY),
            Value::Boolean(response_flags.downstream_protocol_error),
        );
        response_flags_map.insert(
            String::from(UPSTREAM_MAX_STREAM_DURATION_REACHED_KEY),
            Value::Boolean(response_flags.upstream_max_stream_duration_reached),
        );
        response_flags_map.insert(
            String::from(RESPONSE_FROM_CACHE_FILTER_KEY),
            Value::Boolean(response_flags.response_from_cache_filter),
        );
        response_flags_map.insert(
            String::from(NO_FILTER_CONFIG_FOUND_KEY),
            Value::Boolean(response_flags.no_filter_config_found),
        );
        response_flags_map.insert(
            String::from(DURATION_TIMEOUT_KEY),
            Value::Boolean(response_flags.duration_timeout),
        );
        response_flags_map.insert(
            String::from(UPSTREAM_PROTOCOL_ERROR_KEY),
            Value::Boolean(response_flags.upstream_protocol_error),
        );
        response_flags_map.insert(
            String::from(NO_CLUSTER_FOUND_KEY),
            Value::Boolean(response_flags.no_cluster_found),
        );
        response_flags_map.insert(
            String::from(OVERLOAD_MANAGER_KEY),
            Value::Boolean(response_flags.overload_manager),
        );
        response_flags_map.insert(
            String::from(DNS_RESOLUTION_FAILURE_KEY),
            Value::Boolean(response_flags.dns_resolution_failure),
        );
        Value::Object(response_flags_map)
    }
}

impl From<response_flags::Unauthorized> for Value {
    fn from(resp_flags: response_flags::Unauthorized) -> Self {
        let mut reason_map = BTreeMap::new();
        if let Some(reason) = unauthorized::Reason::from_i32(resp_flags.reason) {
            reason_map.insert(
                String::from(REASON_KEY),
                Value::Bytes(reason.as_str_name().into()),
            );
        }
        Value::Object(reason_map)
    }
}

fn prost_any_to_value(a: prost_types::Any) -> Value {
    let mut any_map = BTreeMap::new();
    any_map.insert(String::from(TYPE_URL_KEY), Value::Bytes(a.type_url.into()));
    any_map.insert(String::from(VALUE_KEY), Value::Bytes(a.value.into()));
    Value::Object(any_map)
}

impl From<Metadata> for Value {
    fn from(metadata: Metadata) -> Self {
        let mut metadata_map = BTreeMap::new();
        let mut filter_metadata_map = BTreeMap::new();
        for (key, val) in metadata.filter_metadata {
            filter_metadata_map.insert(key, prost_struct_to_value(val));
        }
        metadata_map.insert(
            String::from(FILTER_METADATA_KEY),
            Value::Object(filter_metadata_map),
        );
        let mut typed_filter_metadata_map = BTreeMap::new();
        for (key, val) in metadata.typed_filter_metadata {
            typed_filter_metadata_map.insert(key, prost_any_to_value(val));
        }
        metadata_map.insert(
            String::from(TYPED_FILTER_METADATA_KEY),
            Value::Object(typed_filter_metadata_map),
        );
        Value::Object(metadata_map)
    }
}

impl From<HttpRequestProperties> for Value {
    fn from(request: HttpRequestProperties) -> Self {
        let mut request_map = BTreeMap::new();
        if let Some(request_method) = RequestMethod::from_i32(request.request_method) {
            request_map.insert(
                String::from(REQUEST_METHOD_KEY),
                Value::Bytes(request_method.as_str_name().into()),
            );
        }
        request_map.insert(
            String::from(SCHEME_KEY),
            Value::Bytes(request.scheme.into()),
        );
        request_map.insert(
            String::from(AUTHORITY_KEY),
            Value::Bytes(request.authority.into()),
        );
        if let Some(port) = request.port {
            request_map.insert(String::from(PORT_KEY), Value::from(port));
        }
        request_map.insert(String::from(PATH_KEY), Value::Bytes(request.path.into()));
        request_map.insert(
            String::from(USER_AGENT_KEY),
            Value::Bytes(request.user_agent.into()),
        );
        request_map.insert(
            String::from(REFERER_KEY),
            Value::Bytes(request.referer.into()),
        );
        request_map.insert(
            String::from(FORWARDED_FOR_KEY),
            Value::Bytes(request.forwarded_for.into()),
        );
        request_map.insert(
            String::from(REQUEST_ID_KEY),
            Value::Bytes(request.request_id.into()),
        );
        request_map.insert(
            String::from(ORIGINAL_PATH_KEY),
            Value::Bytes(request.original_path.into()),
        );
        request_map.insert(
            String::from(REQUEST_HEADERS_BYTES_KEY),
            Value::Integer(request.request_headers_bytes as i64),
        );
        request_map.insert(
            String::from(REQUEST_BODY_BYTES_KEY),
            Value::Integer(request.request_body_bytes as i64),
        );
        let mut request_headers_map = BTreeMap::new();
        for (key, val) in request.request_headers {
            request_headers_map.insert(key, Value::Bytes(val.into()));
        }
        request_map.insert(
            String::from(REQUEST_HEADERS_KEY),
            Value::Object(request_headers_map),
        );
        Value::Object(request_map)
    }
}

impl From<HttpResponseProperties> for Value {
    fn from(response: HttpResponseProperties) -> Self {
        let mut response_map = BTreeMap::new();
        if let Some(response_code) = response.response_code {
            response_map.insert(String::from(RESPONSE_CODE_KEY), Value::from(response_code));
        }
        response_map.insert(
            String::from(RESPONSE_HEADERS_BYTES_KEY),
            Value::Integer(response.response_headers_bytes as i64),
        );
        response_map.insert(
            String::from(RESPONSE_BODY_BYTES_KEY),
            Value::Integer(response.response_body_bytes as i64),
        );
        let mut response_headers_map = BTreeMap::new();
        for (key, val) in response.response_headers {
            response_headers_map.insert(key, Value::Bytes(val.into()));
        }
        response_map.insert(
            String::from(RESPONSE_HEADERS_KEY),
            Value::Object(response_headers_map),
        );
        let mut trailers_map = BTreeMap::new();
        for (key, val) in response.response_trailers {
            trailers_map.insert(key, Value::Bytes(val.into()));
        }
        response_map.insert(
            String::from(RESPONSE_TRAILERS_KEY),
            Value::Object(trailers_map),
        );
        response_map.insert(
            String::from(RESPONSE_CODE_DETAILS_KEY),
            Value::Bytes(response.response_code_details.into()),
        );
        Value::Object(response_map)
    }
}
