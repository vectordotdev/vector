//! Internal events for NetFlow source monitoring and debugging.
//!
//! This module defines all internal events used by the NetFlow source for
//! logging, metrics, and error reporting. These events provide visibility
//! into the source's operation and help with troubleshooting.

use std::net::SocketAddr;
use tracing::{debug, error, info, warn};
use vector_lib::internal_event::{error_stage, error_type, ComponentEventsDropped, InternalEvent, UNINTENTIONAL};
use metrics::counter;

/// NetFlow packet received successfully
#[derive(Debug)]
pub struct NetflowEventsReceived {
    pub count: usize,
    pub byte_size: usize,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for NetflowEventsReceived {
    fn emit(self) {
        debug!(
            message = "NetFlow events received",
            count = self.count,
            byte_size = self.byte_size,
            peer_addr = %self.peer_addr,
        );
        
        counter!(
            "component_received_events_total",
            "peer_addr" => self.peer_addr.to_string(),
        )
        .increment(self.count as u64);
        
        counter!(
            "component_received_event_bytes_total",
            "peer_addr" => self.peer_addr.to_string(),
        )
        .increment(self.byte_size as u64);
    }
}

/// NetFlow packet parsing failed
#[derive(Debug)]
pub struct NetflowParseError<'a> {
    pub error: &'a str,
    pub protocol: &'a str,
    pub peer_addr: SocketAddr,
}

impl<'a> InternalEvent for NetflowParseError<'a> {
    fn emit(self) {
        // Only log as error for critical parsing failures, not missing templates
        if self.error.contains("No template") {
            debug!(
                message = "Template not yet available, data may be buffered",
                error = %self.error,
                protocol = %self.protocol,
                peer_addr = %self.peer_addr,
                error_code = "template_missing",
                error_type = error_type::PARSER_FAILED,
                stage = error_stage::PROCESSING,
                internal_log_rate_limit = true,
            );
        } else {
            error!(
                message = "Failed to parse NetFlow packet",
                error = %self.error,
                protocol = %self.protocol,
                peer_addr = %self.peer_addr,
                error_code = "parse_failed",
                error_type = error_type::PARSER_FAILED,
                stage = error_stage::PROCESSING,
                internal_log_rate_limit = true,
            );
        }
        
        counter!(
            "component_errors_total",
            "error_code" => "parse_failed",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "protocol" => self.protocol.to_string(),
            "peer_addr" => self.peer_addr.to_string(),
        )
        .increment(1);
    }
}

/// Template processing error
#[derive(Debug)]
pub struct NetflowTemplateError<'a> {
    pub error: &'a str,
    pub template_id: u16,
    pub peer_addr: SocketAddr,
}

impl<'a> InternalEvent for NetflowTemplateError<'a> {
    fn emit(self) {
        error!(
            message = "Failed to process NetFlow template",
            error = %self.error,
            template_id = self.template_id,
            peer_addr = %self.peer_addr,
            error_code = "template_error",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        
        counter!(
            "component_errors_total",
            "error_code" => "template_error",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "peer_addr" => self.peer_addr.to_string(),
        )
        .increment(1);
    }
}

/// Field parsing error
#[derive(Debug)]
pub struct NetflowFieldParseError<'a> {
    pub error: &'a str,
    pub field_type: u16,
    pub template_id: u16,
    pub peer_addr: SocketAddr,
}

impl<'a> InternalEvent for NetflowFieldParseError<'a> {
    fn emit(self) {
        error!(
            message = "Failed to parse NetFlow field",
            error = %self.error,
            field_type = self.field_type,
            template_id = self.template_id,
            peer_addr = %self.peer_addr,
            error_code = "field_parse_error",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        
        counter!(
            "component_errors_total",
            "error_code" => "field_parse_error",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "field_type" => self.field_type.to_string(),
            "peer_addr" => self.peer_addr.to_string(),
        )
        .increment(1);
    }
}

/// Events dropped due to parsing issues
#[derive(Debug)]
pub struct NetflowEventsDropped {
    pub count: usize,
    pub reason: &'static str,
}

impl InternalEvent for NetflowEventsDropped {
    fn emit(self) {
        // Reduce noise for template-related drops when buffering is enabled
        if self.reason.contains("No template") {
            debug!(
                message = "NetFlow events dropped - template not available",
                count = self.count,
                reason = %self.reason,
                error_code = "template_missing",
                error_type = error_type::PARSER_FAILED,
                stage = error_stage::PROCESSING,
                internal_log_rate_limit = true,
            );
        } else {
            warn!(
                message = "NetFlow events dropped",
                count = self.count,
                reason = %self.reason,
                error_code = "events_dropped",
                error_type = error_type::PARSER_FAILED,
                stage = error_stage::PROCESSING,
            );
        }
        
        counter!(
            "component_errors_total",
            "error_code" => "events_dropped",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "reason" => self.reason.to_string(),
        )
        .increment(1);
        
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason: self.reason,
        });
    }
}

/// Socket binding error
#[derive(Debug)]
pub struct NetflowBindError {
    pub address: SocketAddr,
    pub error: std::io::Error,
}

impl InternalEvent for NetflowBindError {
    fn emit(self) {
        error!(
            message = "Failed to bind NetFlow socket",
            address = %self.address,
            error = %self.error,
            error_code = "socket_bind_failed",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
        );
        
        counter!(
            "component_errors_total",
            "error_code" => "socket_bind_failed",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
            "address" => self.address.to_string(),
        )
        .increment(1);
    }
}

/// Socket receive error
#[derive(Debug)]
pub struct NetflowReceiveError {
    pub error: std::io::Error,
}

impl InternalEvent for NetflowReceiveError {
    fn emit(self) {
        error!(
            message = "Failed to receive NetFlow packet",
            error = %self.error,
            error_code = "socket_receive_failed",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        
        counter!(
            "component_errors_total",
            "error_code" => "socket_receive_failed",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

/// Multicast group join error
#[derive(Debug)]
pub struct NetflowMulticastJoinError {
    pub group: std::net::Ipv4Addr,
    pub interface: std::net::Ipv4Addr,
    pub error: std::io::Error,
}

impl InternalEvent for NetflowMulticastJoinError {
    fn emit(self) {
        error!(
            message = "Failed to join multicast group",
            group = %self.group,
            interface = %self.interface,
            error = %self.error,
            error_code = "multicast_join_failed",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
        );
        
        counter!(
            "component_errors_total",
            "error_code" => "multicast_join_failed",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
            "group" => self.group.to_string(),
        )
        .increment(1);
    }
}

/// Template cache cleanup completed
#[derive(Debug)]
pub struct TemplateCleanupCompleted {
    pub removed_count: usize,
    pub timeout_seconds: u64,
}

impl InternalEvent for TemplateCleanupCompleted {
    fn emit(self) {
        if self.removed_count > 0 {
            info!(
                message = "Template cache cleanup completed",
                removed_count = self.removed_count,
                timeout_seconds = self.timeout_seconds,
            );
        } else {
            debug!(
                message = "Template cache cleanup completed, no expired templates",
                timeout_seconds = self.timeout_seconds,
            );
        }
        
        counter!(
            "netflow_template_cache_cleanups_total",
        )
        .increment(1);
        
        counter!(
            "netflow_template_cache_expired_total",
        )
        .increment(self.removed_count as u64);
    }
}

/// Template received and cached
#[derive(Debug)]
pub struct TemplateReceived {
    pub template_id: u16,
    pub field_count: u16,
    pub peer_addr: SocketAddr,
    pub observation_domain_id: u32,
    pub protocol: &'static str,
}

/// Buffered records processed when template becomes available
#[derive(Debug)]
pub struct BufferedRecordsProcessed {
    pub template_id: u16,
    pub record_count: usize,
    pub peer_addr: SocketAddr,
    pub observation_domain_id: u32,
}

impl InternalEvent for TemplateReceived {
    fn emit(self) {
        debug!(
            message = "Template received and cached",
            template_id = self.template_id,
            field_count = self.field_count,
            peer_addr = %self.peer_addr,
            observation_domain_id = self.observation_domain_id,
            protocol = self.protocol,
        );
        
        counter!(
            "netflow_templates_received_total",
            "protocol" => self.protocol.to_string(),
            "peer_addr" => self.peer_addr.to_string(),
        )
        .increment(1);
    }
}

impl InternalEvent for BufferedRecordsProcessed {
    fn emit(self) {
        debug!(
            message = "Buffered records processed with new template",
            template_id = self.template_id,
            record_count = self.record_count,
            peer_addr = %self.peer_addr,
            observation_domain_id = self.observation_domain_id,
        );
        
        counter!(
            "netflow_buffered_records_processed_total",
            "template_id" => self.template_id.to_string(),
            "peer_addr" => self.peer_addr.to_string(),
        )
        .increment(self.record_count as u64);
    }
}

/// Template cache statistics
#[derive(Debug)]
pub struct TemplateCacheStats {
    pub cache_size: usize,
    pub max_size: usize,
    pub hit_ratio: f64,
    pub total_hits: u64,
    pub total_misses: u64,
}

impl InternalEvent for TemplateCacheStats {
    fn emit(self) {
        debug!(
            message = "Template cache statistics",
            cache_size = self.cache_size,
            max_size = self.max_size,
            hit_ratio = self.hit_ratio,
            total_hits = self.total_hits,
            total_misses = self.total_misses,
        );
        
        metrics::gauge!(
            "netflow_template_cache_size",
        )
        .set(self.cache_size as f64);
        
        metrics::gauge!(
            "netflow_template_cache_hit_ratio",
        )
        .set(self.hit_ratio);
    }
}

/// Data record parsed successfully
#[derive(Debug)]
pub struct DataRecordParsed {
    pub template_id: u16,
    pub fields_parsed: usize,
    pub record_size: usize,
    pub peer_addr: SocketAddr,
    pub protocol: &'static str,
}

impl InternalEvent for DataRecordParsed {
    fn emit(self) {
        debug!(
            message = "Data record parsed successfully",
            template_id = self.template_id,
            fields_parsed = self.fields_parsed,
            record_size = self.record_size,
            peer_addr = %self.peer_addr,
            protocol = self.protocol,
        );
        
        counter!(
            "netflow_records_parsed_total",
            "protocol" => self.protocol.to_string(),
            "peer_addr" => self.peer_addr.to_string(),
        )
        .increment(1);
        
        counter!(
            "netflow_fields_parsed_total",
            "protocol" => self.protocol.to_string(),
        )
        .increment(self.fields_parsed as u64);
    }
}

/// Enterprise field encountered
#[derive(Debug)]
pub struct EnterpriseFieldEncountered {
    pub enterprise_id: u32,
    pub field_type: u16,
    pub field_name: String,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for EnterpriseFieldEncountered {
    fn emit(self) {
        debug!(
            message = "Enterprise field encountered",
            enterprise_id = self.enterprise_id,
            field_type = self.field_type,
            field_name = %self.field_name,
            peer_addr = %self.peer_addr,
        );
        
        counter!(
            "netflow_enterprise_fields_total",
            "enterprise_id" => self.enterprise_id.to_string(),
            "field_name" => self.field_name,
        )
        .increment(1);
    }
}

/// Unknown enterprise field encountered
#[derive(Debug)]
pub struct UnknownEnterpriseField {
    pub enterprise_id: u32,
    pub field_type: u16,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for UnknownEnterpriseField {
    fn emit(self) {
        warn!(
            message = "Unknown enterprise field encountered",
            enterprise_id = self.enterprise_id,
            field_type = self.field_type,
            peer_addr = %self.peer_addr,
            internal_log_rate_limit = true,
        );
        
        counter!(
            "netflow_unknown_enterprise_fields_total",
            "enterprise_id" => self.enterprise_id.to_string(),
        )
        .increment(1);
    }
}

/// Protocol version mismatch
#[derive(Debug)]
pub struct ProtocolVersionMismatch {
    pub expected: u16,
    pub received: u16,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for ProtocolVersionMismatch {
    fn emit(self) {
        warn!(
            message = "Protocol version mismatch",
            expected = self.expected,
            received = self.received,
            peer_addr = %self.peer_addr,
            error_code = "version_mismatch",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        
        counter!(
            "component_errors_total",
            "error_code" => "version_mismatch",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "expected_version" => self.expected.to_string(),
            "received_version" => self.received.to_string(),
        )
        .increment(1);
    }
}

/// Packet too large error
#[derive(Debug)]
pub struct PacketTooLarge {
    pub packet_size: usize,
    pub max_size: usize,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for PacketTooLarge {
    fn emit(self) {
        warn!(
            message = "Packet too large, truncating",
            packet_size = self.packet_size,
            max_size = self.max_size,
            peer_addr = %self.peer_addr,
            error_code = "packet_too_large",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        
        counter!(
            "component_errors_total",
            "error_code" => "packet_too_large",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

/// Source startup information
#[derive(Debug)]
pub struct NetflowSourceStarted {
    pub address: SocketAddr,
    pub protocols: Vec<String>,
    pub max_templates: usize,
    pub template_timeout: u64,
}

impl InternalEvent for NetflowSourceStarted {
    fn emit(self) {
        info!(
            message = "NetFlow source started",
            address = %self.address,
            protocols = ?self.protocols,
            max_templates = self.max_templates,
            template_timeout = self.template_timeout,
        );
        
        counter!(
            "netflow_source_starts_total",
        )
        .increment(1);
    }
}

/// Source shutdown information
#[derive(Debug)]
pub struct NetflowSourceStopped {
    pub address: SocketAddr,
    pub runtime_seconds: u64,
}

impl InternalEvent for NetflowSourceStopped {
    fn emit(self) {
        info!(
            message = "NetFlow source stopped",
            address = %self.address,
            runtime_seconds = self.runtime_seconds,
        );
        
        counter!(
            "netflow_source_stops_total",
        )
        .increment(1);
    }
}

/// Configuration validation error
#[derive(Debug)]
pub struct ConfigValidationError {
    pub errors: Vec<String>,
}

impl InternalEvent for ConfigValidationError {
    fn emit(self) {
        error!(
            message = "NetFlow configuration validation failed",
            errors = ?self.errors,
            error_code = "config_validation_failed",
            error_type = error_type::CONFIGURATION_FAILED,
            stage = error_stage::RECEIVING,
        );
        
        counter!(
            "component_errors_total",
            "error_code" => "config_validation_failed",
            "error_type" => error_type::CONFIGURATION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

/// Variable-length field parsing
#[derive(Debug)]
pub struct VariableLengthFieldParsed {
    pub field_type: u16,
    pub actual_length: usize,
    pub template_id: u16,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for VariableLengthFieldParsed {
    fn emit(self) {
        debug!(
            message = "Variable-length field parsed",
            field_type = self.field_type,
            actual_length = self.actual_length,
            template_id = self.template_id,
            peer_addr = %self.peer_addr,
        );
        
        counter!(
            "netflow_variable_length_fields_total",
        )
        .increment(1);
    }
}

/// Flow statistics summary
#[derive(Debug)]
pub struct FlowStatsSummary {
    pub total_flows: u64,
    pub total_packets: u64,
    pub total_bytes: u64,
    pub unique_peers: usize,
    pub active_templates: usize,
}

impl InternalEvent for FlowStatsSummary {
    fn emit(self) {
        info!(
            message = "Flow statistics summary",
            total_flows = self.total_flows,
            total_packets = self.total_packets,
            total_bytes = self.total_bytes,
            unique_peers = self.unique_peers,
            active_templates = self.active_templates,
        );
        
        metrics::gauge!(
            "netflow_total_flows",
        )
        .set(self.total_flows as f64);
        
        metrics::gauge!(
            "netflow_unique_peers",
        )
        .set(self.unique_peers as f64);
        
        metrics::gauge!(
            "netflow_active_templates",
        )
        .set(self.active_templates as f64);
    }
}

/// Protocol-specific events
#[derive(Debug)]
pub struct ProtocolSpecificEvent {
    pub protocol: &'static str,
    pub event_type: &'static str,
    pub details: String,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for ProtocolSpecificEvent {
    fn emit(self) {
        debug!(
            message = "Protocol-specific event",
            protocol = self.protocol,
            event_type = self.event_type,
            details = %self.details,
            peer_addr = %self.peer_addr,
        );
        
        counter!(
            "netflow_protocol_events_total",
            "protocol" => self.protocol.to_string(),
            "event_type" => self.event_type.to_string(),
        )
        .increment(1);
    }
}

/// Memory usage warning
#[derive(Debug)]
pub struct MemoryUsageWarning {
    pub component: &'static str,
    pub current_usage: usize,
    pub threshold: usize,
}

impl InternalEvent for MemoryUsageWarning {
    fn emit(self) {
        warn!(
            message = "Memory usage warning",
            component = self.component,
            current_usage = self.current_usage,
            threshold = self.threshold,
            error_code = "memory_usage_high",
            error_type = error_type::CONFIGURATION_FAILED,
            stage = error_stage::PROCESSING,
        );
        
        counter!(
            "component_errors_total",
            "error_code" => "memory_usage_high",
            "error_type" => error_type::CONFIGURATION_FAILED,
            "stage" => error_stage::PROCESSING,
            "component" => self.component.to_string(),
        )
        .increment(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_peer_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 2055)
    }

    #[test]
    fn test_events_can_be_created() {
        // Test that all events can be instantiated without panicking
        let peer_addr = test_peer_addr();

        let _events = vec![
            Box::new(NetflowEventsReceived {
                count: 10,
                byte_size: 1500,
                peer_addr,
            }) as Box<dyn std::fmt::Debug>,
            
            Box::new(NetflowParseError {
                error: "test error",
                protocol: "netflow_v5",
                peer_addr,
            }),
            
            Box::new(NetflowTemplateError {
                error: "template error",
                template_id: 256,
                peer_addr,
            }),
            
            Box::new(NetflowFieldParseError {
                error: "field error",
                field_type: 1,
                template_id: 256,
                peer_addr,
            }),
            
            Box::new(NetflowEventsDropped {
                count: 5,
                reason: "no template",
            }),
            
            Box::new(TemplateReceived {
                template_id: 256,
                field_count: 10,
                peer_addr,
                observation_domain_id: 1,
                protocol: "ipfix",
            }),
            
            Box::new(EnterpriseFieldEncountered {
                enterprise_id: 23867,
                field_type: 1,
                field_name: "clientIPv4Address".to_string(),
                peer_addr,
            }),
        ];

        // If we get here without panicking, the test passes
        assert!(!_events.is_empty());
    }

    #[test]
    fn test_event_emission() {
        // Test that events can be emitted without panicking
        let peer_addr = test_peer_addr();

        // These should not panic
        NetflowEventsReceived {
            count: 1,
            byte_size: 100,
            peer_addr,
        }.emit();

        TemplateCleanupCompleted {
            removed_count: 5,
            timeout_seconds: 3600,
        }.emit();

        FlowStatsSummary {
            total_flows: 1000,
            total_packets: 50000,
            total_bytes: 1000000,
            unique_peers: 10,
            active_templates: 50,
        }.emit();
    }

    #[test]
    fn test_error_events() {
        let peer_addr = test_peer_addr();

        // Test error events
        NetflowParseError {
            error: "Invalid packet format",
            protocol: "netflow_v9",
            peer_addr,
        }.emit();

        NetflowTemplateError {
            error: "Template field count mismatch",
            template_id: 512,
            peer_addr,
        }.emit();

        ProtocolVersionMismatch {
            expected: 9,
            received: 5,
            peer_addr,
        }.emit();
    }

    #[test]
    fn test_statistics_events() {
        let peer_addr = test_peer_addr();

        TemplateCacheStats {
            cache_size: 100,
            max_size: 1000,
            hit_ratio: 0.95,
            total_hits: 950,
            total_misses: 50,
        }.emit();

        DataRecordParsed {
            template_id: 256,
            fields_parsed: 15,
            record_size: 60,
            peer_addr,
            protocol: "ipfix",
        }.emit();
    }

    #[test]
    fn test_lifecycle_events() {
        let address = test_peer_addr();

        NetflowSourceStarted {
            address,
            protocols: vec!["netflow_v5".to_string(), "ipfix".to_string()],
            max_templates: 1000,
            template_timeout: 3600,
        }.emit();

        NetflowSourceStopped {
            address,
            runtime_seconds: 86400,
        }.emit();
    }

    #[test]
    fn test_configuration_events() {
        ConfigValidationError {
            errors: vec![
                "max_packet_size must be > 0".to_string(),
                "invalid protocol specified".to_string(),
            ],
        }.emit();
    }

    #[test]
    fn test_enterprise_field_events() {
        let peer_addr = test_peer_addr();

        EnterpriseFieldEncountered {
            enterprise_id: 9,
            field_type: 1001,
            field_name: "cisco_application_id".to_string(),
            peer_addr,
        }.emit();

        UnknownEnterpriseField {
            enterprise_id: 99999,
            field_type: 5000,
            peer_addr,
        }.emit();
    }

    #[test]
    fn test_variable_length_field_event() {
        let peer_addr = test_peer_addr();

        VariableLengthFieldParsed {
            field_type: 82,
            actual_length: 256,
            template_id: 512,
            peer_addr,
        }.emit();
    }

    #[test]
    fn test_memory_warning_event() {
        MemoryUsageWarning {
            component: "template_cache",
            current_usage: 104857600, // 100MB
            threshold: 83886080,      // 80MB
        }.emit();
    }

    #[test]
    fn test_protocol_specific_event() {
        let peer_addr = test_peer_addr();

        ProtocolSpecificEvent {
            protocol: "sflow",
            event_type: "sample_rate_change",
            details: "Sample rate changed from 1000 to 2000".to_string(),
            peer_addr,
        }.emit();
    }
}