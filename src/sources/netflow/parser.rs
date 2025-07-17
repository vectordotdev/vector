//! NetFlow, IPFIX, and sFlow parsing logic for the netflow source.

use crate::event::{Event, LogEvent};
use crate::sources::netflow::template_cache::{Template, TemplateField, TemplateCache, cache_put, cache_get};
use base64::Engine;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::OnceLock;

// Protocol-specific constants
pub const MAX_SET_LEN: usize = 65535;
pub const NETFLOW_V5_HEADER_SIZE: usize = 24;
pub const NETFLOW_V5_RECORD_SIZE: usize = 48;
pub const NETFLOW_V9_HEADER_SIZE: usize = 20;
pub const IPFIX_HEADER_SIZE: usize = 16;
pub const SFLOW_HEADER_SIZE: usize = 28;
pub const IPFIX_VERSION: u16 = 10;
pub const NETFLOW_V5_VERSION: u16 = 5;
pub const NETFLOW_V9_VERSION: u16 = 9;
pub const SFLOW_VERSION: u32 = 5;

// NetFlow v5 header structure
#[derive(Debug)]
pub struct NetflowHeader {
    pub version: u16,
    pub count: u16,
    pub sys_uptime: u32,
    pub unix_secs: u32,
    pub unix_nsecs: u32,
    pub flow_sequence: u32,
    pub engine_type: u8,
    pub engine_id: u8,
    pub sampling_interval: u16,
}

// NetFlow v5 record structure
#[derive(Debug)]
pub struct NetflowV5Record {
    pub src_addr: u32,
    pub dst_addr: u32,
    pub nexthop: u32,
    pub input: u16,
    pub output: u16,
    pub packets: u32,
    pub octets: u32,
    pub first: u32,
    pub last: u32,
    pub src_port: u16,
    pub dst_port: u16,
    pub pad1: u8,
    pub tcp_flags: u8,
    pub protocol: u8,
    pub tos: u8,
    pub src_as: u16,
    pub dst_as: u16,
    pub src_mask: u8,
    pub dst_mask: u8,
    pub pad2: u16,
}

// NetFlow v9 header structure
#[derive(Debug)]
pub struct NetflowV9Header {
    pub version: u16,
    pub count: u16,
    pub sys_uptime: u32,
    pub unix_secs: u32,
    pub flow_sequence: u32,
    pub source_id: u32,
}

// IPFIX header structure
#[derive(Debug)]
pub struct IpfixHeader {
    pub version: u16,
    pub length: u16,
    pub export_time: u32,
    pub sequence_number: u32,
    pub observation_domain_id: u32,
}

// sFlow header structure
#[derive(Debug)]
pub struct SflowHeader {
    pub version: u32,
    pub agent_address_type: u32,
    pub agent_address: u32,
    pub sub_agent_id: u32,
    pub sequence_number: u32,
    pub sys_uptime: u32,
    pub num_samples: u32,
}

// IPFIX field name mapping
static IPFIX_FIELD_MAP: OnceLock<HashMap<u16, &'static str>> = OnceLock::new();

pub fn ipfix_field_names() -> &'static HashMap<u16, &'static str> {
    IPFIX_FIELD_MAP.get_or_init(|| {
        let mut map = HashMap::new();
        
        // Standard IPFIX Information Elements (RFC 5102, RFC 5103, RFC 5104, etc.)
        // Basic flow fields
        map.insert(1, "octetDeltaCount");
        map.insert(2, "packetDeltaCount");
        map.insert(3, "deltaFlowCount");
        map.insert(4, "protocolIdentifier");
        map.insert(5, "ipClassOfService");
        map.insert(6, "tcpControlBits");
        map.insert(7, "sourceTransportPort");
        map.insert(8, "sourceIPv4Address");
        map.insert(9, "sourceIPv4PrefixLength");
        map.insert(10, "ingressInterface");
        map.insert(11, "destinationTransportPort");
        map.insert(12, "destinationIPv4Address");
        map.insert(13, "destinationIPv4PrefixLength");
        map.insert(14, "egressInterface");
        map.insert(15, "ipNextHopIPv4Address");
        map.insert(16, "bgpSourceAsNumber");
        map.insert(17, "bgpDestinationAsNumber");
        map.insert(18, "bgpNextHopIPv4Address");
        map.insert(19, "postMCastPacketDeltaCount");
        map.insert(20, "postMCastOctetDeltaCount");
        map.insert(21, "flowEndSysUpTime");
        map.insert(22, "flowStartSysUpTime");
        map.insert(23, "postOctetDeltaCount");
        map.insert(24, "postPacketDeltaCount");
        map.insert(25, "minimumIpTotalLength");
        map.insert(26, "maximumIpTotalLength");
        map.insert(27, "sourceIPv6Address");
        map.insert(28, "destinationIPv6Address");
        map.insert(29, "sourceIPv6PrefixLength");
        map.insert(30, "destinationIPv6PrefixLength");
        map.insert(31, "flowLabelIPv6");
        map.insert(32, "icmpTypeCodeIPv4");
        map.insert(33, "igmpType");
        map.insert(34, "samplingInterval");
        map.insert(35, "samplingAlgorithm");
        map.insert(36, "flowActiveTimeout");
        map.insert(37, "flowInactiveTimeout");
        map.insert(38, "engineType");
        map.insert(39, "engineId");
        map.insert(40, "exportedOctetTotalCount");
        map.insert(41, "exportedMessageTotalCount");
        map.insert(42, "exportedFlowRecordTotalCount");
        map.insert(43, "ipv4RouterSc");
        map.insert(44, "sourceIPv4Prefix");
        map.insert(45, "destinationIPv4Prefix");
        map.insert(46, "mplsTopLabelType");
        map.insert(47, "mplsTopLabelIPv4Address");
        map.insert(48, "samplerId");
        map.insert(49, "samplerMode");
        map.insert(50, "samplerRandomInterval");
        map.insert(51, "classId");
        map.insert(52, "minimumTTL");
        map.insert(53, "maximumTTL");
        map.insert(54, "fragmentIdentification");
        map.insert(55, "postIpClassOfService");
        map.insert(56, "sourceMacAddress");
        map.insert(57, "postDestinationMacAddress");
        map.insert(58, "vlanId");
        map.insert(59, "postVlanId");
        map.insert(60, "ipVersion");
        map.insert(61, "flowDirection");
        map.insert(62, "ipNextHopIPv6Address");
        map.insert(63, "bgpNextHopIPv6Address");
        map.insert(64, "ipv6ExtensionHeaders");
        
        // Additional fields for comprehensive coverage
        map.insert(65, "forwardingStatus");
        map.insert(66, "mplsVpnRouteDistinguisher");
        map.insert(67, "mplsTopLabelPrefixLength");
        map.insert(68, "srcTrafficIndex");
        map.insert(69, "dstTrafficIndex");

        map.insert(70, "mplsTopLabelStackSection");
        map.insert(71, "mplsLabelStackSection2");
        map.insert(72, "mplsLabelStackSection3");
        map.insert(73, "mplsLabelStackSection4");
        map.insert(74, "mplsLabelStackSection5");
        map.insert(75, "mplsLabelStackSection6");
        map.insert(76, "mplsLabelStackSection7");
        map.insert(77, "mplsLabelStackSection8");
        map.insert(78, "mplsLabelStackSection9");
        map.insert(79, "mplsLabelStackSection10");
        map.insert(80, "destinationMacAddress");
        map.insert(81, "postSourceMacAddress");
        map.insert(82, "interfaceName");
        map.insert(83, "interfaceDescription");
        map.insert(84, "samplerName");
        map.insert(85, "octetTotalCount");
        map.insert(86, "packetTotalCount");
        map.insert(87, "flagsAndSamplerId");
        map.insert(88, "fragmentOffset");
        map.insert(89, "forwardingStatus");
        map.insert(90, "mplsVpnRouteDistinguisher");
        map.insert(91, "mplsTopLabelPrefixLength");
        map.insert(92, "srcTrafficIndex");
        map.insert(93, "dstTrafficIndex");
        map.insert(94, "applicationDescription");
        map.insert(95, "applicationId");
        map.insert(96, "applicationName");
        map.insert(97, "samplingInterval");
        map.insert(98, "postIpDiffServCodePoint");
        map.insert(99, "multicastReplicationFactor");
        map.insert(100, "flowInactiveTimeout");
        map.insert(101, "engineType");
        map.insert(102, "engineId");
        map.insert(103, "exportedOctetTotalCount");
        map.insert(104, "exportedMessageTotalCount");
        map.insert(105, "exportedFlowRecordTotalCount");
        map.insert(106, "ipv4RouterSc");
        map.insert(107, "sourceIPv4Prefix");
        map.insert(108, "destinationIPv4Prefix");
        map.insert(109, "mplsTopLabelStackSection");
        map.insert(110, "mplsLabelStackSection2");
        map.insert(111, "mplsLabelStackSection3");
        map.insert(112, "mplsLabelStackSection4");
        map.insert(113, "mplsLabelStackSection5");
        map.insert(114, "mplsLabelStackSection6");
        map.insert(115, "mplsLabelStackSection7");
        map.insert(116, "mplsLabelStackSection8");
        map.insert(117, "mplsLabelStackSection9");
        map.insert(118, "mplsLabelStackSection10");
        map.insert(119, "destinationMacAddress");
        map.insert(120, "postSourceMacAddress");
        map.insert(121, "interfaceName");
        map.insert(122, "interfaceDescription");
        map.insert(123, "samplerName");
        map.insert(124, "octetTotalCount");
        map.insert(125, "packetTotalCount");
        map.insert(126, "flagsAndSamplerId");
        map.insert(127, "fragmentOffset");
        map.insert(128, "forwardingStatus");
        map.insert(129, "mplsVpnRouteDistinguisher");
        map.insert(130, "mplsTopLabelPrefixLength");
        map.insert(131, "srcTrafficIndex");
        map.insert(132, "dstTrafficIndex");
        map.insert(133, "applicationDescription");
        map.insert(134, "applicationId");
        map.insert(135, "applicationName");
        map.insert(136, "postIpDiffServCodePoint");
        map.insert(137, "multicastReplicationFactor");
        map.insert(138, "classificationEngineId");
        map.insert(139, "bgpNextAdjacentAsNumber");
        map.insert(140, "bgpPrevAdjacentAsNumber");
        map.insert(141, "exporterIPv4Address");
        map.insert(142, "exporterIPv6Address");
        map.insert(143, "droppedOctetDeltaCount");
        map.insert(144, "droppedPacketDeltaCount");
        map.insert(145, "droppedOctetTotalCount");
        map.insert(146, "droppedPacketTotalCount");
        map.insert(147, "flowKeyIndicator");
        map.insert(148, "flowId");
        map.insert(149, "observationDomainId");
        map.insert(150, "flowStartSeconds");
        map.insert(151, "flowEndSeconds");
        map.insert(152, "flowStartMilliseconds");
        map.insert(153, "flowEndMilliseconds");
        map.insert(154, "flowStartMicroseconds");
        map.insert(155, "flowEndMicroseconds");
        map.insert(156, "flowStartNanoseconds");
        map.insert(157, "flowEndNanoseconds");
        map.insert(158, "flowStartDeltaMicroseconds");
        map.insert(159, "flowEndDeltaMicroseconds");
        map.insert(160, "systemInitTimeMilliseconds");
        map.insert(161, "flowDurationMilliseconds");
        map.insert(162, "flowDurationMicroseconds");
        map.insert(163, "notSentPacketTotalCount");
        map.insert(164, "notSentOctetTotalCount");
        map.insert(165, "destinationIPv4Prefix");
        map.insert(166, "sourceIPv4Prefix");
        map.insert(167, "mplsTopLabelStackSection");
        map.insert(168, "mplsLabelStackSection2");
        map.insert(169, "mplsLabelStackSection3");
        map.insert(170, "mplsLabelStackSection4");
        map.insert(171, "mplsLabelStackSection5");
        map.insert(172, "mplsLabelStackSection6");
        map.insert(173, "mplsLabelStackSection7");
        map.insert(174, "mplsLabelStackSection8");
        map.insert(175, "mplsLabelStackSection9");
        map.insert(176, "mplsLabelStackSection10");
        map.insert(177, "destinationMacAddress");
        map.insert(178, "postSourceMacAddress");
        map.insert(179, "interfaceName");
        map.insert(180, "interfaceDescription");
        map.insert(181, "samplerName");
        map.insert(182, "tcpSourcePort");
        map.insert(183, "tcpDestinationPort");
        map.insert(184, "flagsAndSamplerId");
        map.insert(185, "fragmentOffset");
        map.insert(186, "forwardingStatus");
        map.insert(187, "mplsVpnRouteDistinguisher");
        map.insert(188, "mplsTopLabelPrefixLength");
        map.insert(189, "srcTrafficIndex");
        map.insert(190, "dstTrafficIndex");
        map.insert(191, "applicationDescription");
        map.insert(192, "applicationId");
        map.insert(193, "applicationName");
        map.insert(194, "postIpDiffServCodePoint");
        map.insert(195, "multicastReplicationFactor");
        map.insert(196, "classificationEngineId");
        map.insert(197, "bgpNextAdjacentAsNumber");
        map.insert(198, "bgpPrevAdjacentAsNumber");
        map.insert(199, "exporterIPv4Address");
        map.insert(200, "exporterIPv6Address");
        
        // Additional fields for comprehensive coverage
        map.insert(201, "droppedOctetDeltaCount");
        map.insert(202, "droppedPacketDeltaCount");
        map.insert(203, "droppedOctetTotalCount");
        map.insert(204, "droppedPacketTotalCount");
        map.insert(205, "flowKeyIndicator");
        map.insert(206, "upstreamSessionId");
        map.insert(207, "downstreamSessionId");
        map.insert(208, "ipTtl");
        map.insert(209, "nextHopIPv4Address");
        map.insert(210, "nextHopIPv6Address");
        map.insert(211, "ipV6ExtensionHeaders");
        map.insert(212, "mplsTopLabelExp");
        map.insert(213, "flowLabel");
        map.insert(214, "icmpTypeCodeIPv6");
        map.insert(215, "icmpType");
        map.insert(216, "icmpCode");
        map.insert(217, "exporterTransportPort");
        map.insert(218, "tcpSynTotalCount");
        map.insert(219, "samplingAlgorithm");
        map.insert(220, "flowActiveTimeout");
        map.insert(221, "flowInactiveTimeout");
        map.insert(222, "engineType");
        map.insert(223, "engineId");
        map.insert(224, "exportedOctetTotalCount");
        map.insert(225, "exportedMessageTotalCount");
        map.insert(226, "exportedFlowRecordTotalCount");
        map.insert(227, "ipv4RouterSc");
        map.insert(228, "sourceIPv4Prefix");
        map.insert(229, "destinationIPv4Prefix");
        map.insert(230, "mplsTopLabelStackSection");
        map.insert(231, "mplsLabelStackSection2");
        map.insert(232, "mplsLabelStackSection3");
        map.insert(233, "mplsLabelStackSection4");
        map.insert(234, "mplsLabelStackSection5");
        map.insert(235, "mplsLabelStackSection6");
        map.insert(236, "mplsLabelStackSection7");
        map.insert(237, "mplsLabelStackSection8");
        map.insert(238, "mplsLabelStackSection9");
        map.insert(239, "mplsLabelStackSection10");
        map.insert(240, "destinationMacAddress");
        map.insert(241, "postSourceMacAddress");
        map.insert(242, "interfaceName");
        map.insert(243, "interfaceDescription");
        map.insert(244, "samplerName");
        map.insert(245, "octetTotalCount");
        map.insert(246, "packetTotalCount");
        map.insert(247, "flagsAndSamplerId");
        map.insert(248, "fragmentOffset");
        map.insert(249, "forwardingStatus");
        map.insert(250, "mplsVpnRouteDistinguisher");
        map.insert(251, "mplsTopLabelPrefixLength");
        map.insert(252, "srcTrafficIndex");
        map.insert(253, "dstTrafficIndex");
        map.insert(254, "applicationDescription");
        map.insert(255, "applicationId");
        
        // Enterprise-specific fields (common vendors)
        // Cisco fields (enterprise 9)
        map.insert(0x8001, "ciscoConnectionId");
        map.insert(0x8002, "ciscoConnectionType");
        map.insert(0x8003, "ciscoConnectionTimeout");
        map.insert(0x8004, "ciscoConnectionDirection");
        map.insert(0x8005, "ciscoConnectionInitiation");
        map.insert(0x8006, "ciscoConnectionIpv4Address");
        map.insert(0x8007, "ciscoConnectionIpv6Address");
        map.insert(0x8008, "ciscoConnectionTransportProtocol");
        map.insert(0x8009, "ciscoConnectionApplicationProtocol");
        map.insert(0x800A, "ciscoConnectionApplicationName");
        
        // Juniper fields (enterprise 2636)
        map.insert(0x800B, "juniperSrcInterface");
        map.insert(0x800C, "juniperDstInterface");
        map.insert(0x800D, "juniperSrcInterfaceLogical");
        map.insert(0x800E, "juniperDstInterfaceLogical");
        map.insert(0x800F, "juniperSrcInterfacePhysical");
        map.insert(0x8010, "juniperDstInterfacePhysical");
        map.insert(0x8011, "juniperSrcInterfaceLogical");
        map.insert(0x8012, "juniperDstInterfaceLogical");
        map.insert(0x8013, "juniperSrcInterfacePhysical");
        map.insert(0x8014, "juniperDstInterfacePhysical");
        
        // Huawei fields (enterprise 2011)
        map.insert(0x8015, "huaweiSrcInterface");
        map.insert(0x8016, "huaweiDstInterface");
        map.insert(0x8017, "huaweiSrcInterfaceLogical");
        map.insert(0x8018, "huaweiDstInterfaceLogical");
        map.insert(0x8019, "huaweiSrcInterfacePhysical");
        map.insert(0x801A, "huaweiDstInterfacePhysical");
        map.insert(0x801B, "huaweiSrcInterfaceLogical");
        map.insert(0x801C, "huaweiDstInterfaceLogical");
        map.insert(0x801D, "huaweiSrcInterfacePhysical");
        map.insert(0x801E, "huaweiDstInterfacePhysical");
        
        // Nokia/Alcatel-Lucent fields (enterprise 6527)
        map.insert(0x801F, "nokiaSrcInterface");
        map.insert(0x8020, "nokiaDstInterface");
        map.insert(0x8021, "nokiaSrcInterfaceLogical");
        map.insert(0x8022, "nokiaDstInterfaceLogical");
        map.insert(0x8023, "nokiaSrcInterfacePhysical");
        map.insert(0x8024, "nokiaDstInterfacePhysical");
        map.insert(0x8025, "nokiaSrcInterfaceLogical");
        map.insert(0x8026, "nokiaDstInterfaceLogical");
        map.insert(0x8027, "nokiaSrcInterfacePhysical");
        map.insert(0x8028, "nokiaDstInterfacePhysical");
        
        // Arista fields (enterprise 30065)
        map.insert(0x8029, "aristaSrcInterface");
        map.insert(0x802A, "aristaDstInterface");
        map.insert(0x802B, "aristaSrcInterfaceLogical");
        map.insert(0x802C, "aristaDstInterfaceLogical");
        map.insert(0x802D, "aristaSrcInterfacePhysical");
        map.insert(0x802E, "aristaDstInterfacePhysical");
        map.insert(0x802F, "aristaSrcInterfaceLogical");
        map.insert(0x8030, "aristaDstInterfaceLogical");
        map.insert(0x8031, "aristaSrcInterfacePhysical");
        map.insert(0x8032, "aristaDstInterfacePhysical");
        
        // Brocade fields (enterprise 1916)
        map.insert(0x8033, "brocadeSrcInterface");
        map.insert(0x8034, "brocadeDstInterface");
        map.insert(0x8035, "brocadeSrcInterfaceLogical");
        map.insert(0x8036, "brocadeDstInterfaceLogical");
        map.insert(0x8037, "brocadeSrcInterfacePhysical");
        map.insert(0x8038, "brocadeDstInterfacePhysical");
        map.insert(0x8039, "brocadeSrcInterfaceLogical");
        map.insert(0x803A, "brocadeDstInterfaceLogical");
        map.insert(0x803B, "brocadeSrcInterfacePhysical");
        map.insert(0x803C, "brocadeDstInterfacePhysical");
        
        // Extreme Networks fields (enterprise 1916)
        map.insert(0x803D, "extremeSrcInterface");
        map.insert(0x803E, "extremeDstInterface");
        map.insert(0x803F, "extremeSrcInterfaceLogical");
        map.insert(0x8040, "extremeDstInterfaceLogical");
        map.insert(0x8041, "extremeSrcInterfacePhysical");
        map.insert(0x8042, "extremeDstInterfacePhysical");
        map.insert(0x8043, "extremeSrcInterfaceLogical");
        map.insert(0x8044, "extremeDstInterfaceLogical");
        map.insert(0x8045, "extremeSrcInterfacePhysical");
        map.insert(0x8046, "extremeDstInterfacePhysical");
        
        // HPE Aruba Networking EdgeConnect SD-WAN fields (enterprise 23867)
        // IPv4 Address fields
        map.insert(0x8000 + 1, "clientIPv4Address");
        map.insert(0x8000 + 2, "serverIPv4Address");
        map.insert(0x8000 + 7, "connectionInitiator");
        
        // Unsigned 8-bit fields
        map.insert(0x8000 + 9, "connectionNumberOfConnections");
        map.insert(0x8000 + 10, "connectionServerResponsesCount");
        map.insert(0x8000 + 21, "connectionTransactionCompleteCount");
        
        // Unsigned 32-bit fields (microseconds)
        map.insert(0x8000 + 11, "connectionServerResponseDelay");
        map.insert(0x8000 + 12, "connectionNetworkToServerDelay");
        map.insert(0x8000 + 13, "connectionNetworkToClientDelay");
        map.insert(0x8000 + 14, "connectionClientPacketRetransmissionCount");
        map.insert(0x8000 + 15, "connectionClientToServerNetworkDelay");
        map.insert(0x8000 + 16, "connectionApplicationDelay");
        map.insert(0x8000 + 17, "connectionClientToServerResponseDelay");
        map.insert(0x8000 + 18, "connectionTransactionDuration");
        map.insert(0x8000 + 19, "connectionTransactionDurationMin");
        map.insert(0x8000 + 20, "connectionTransactionDurationMax");
        
        // Unsigned 64-bit fields (octets/packets)
        map.insert(0x8000 + 3, "connectionServerOctetDeltaCount");
        map.insert(0x8000 + 4, "connectionServerPacketDeltaCount");
        map.insert(0x8000 + 5, "connectionClientOctetDeltaCount");
        map.insert(0x8000 + 6, "connectionClientPacketDeltaCount");
        
        // String fields (variable length)
        map.insert(0x8000 + 8, "applicationHttpHost");
        map.insert(0x8000 + 22, "fromZone");
        map.insert(0x8000 + 23, "toZone");
        map.insert(0x8000 + 24, "tag");
        map.insert(0x8000 + 25, "overlay");
        map.insert(0x8000 + 26, "direction");
        map.insert(0x8000 + 27, "applicationCategory");
        
        // Legacy fields (from previous documentation)
        map.insert(0x8000 + 10001, "overlayTunnelID");
        map.insert(0x8000 + 10002, "policyMatchID");
        map.insert(0x8000 + 10003, "applianceName");
        map.insert(0x8000 + 10004, "WANInterfaceID");
        map.insert(0x8000 + 10005, "QOSQueueID");
        map.insert(0x8000 + 10006, "linkQualityMetrics");
        
        // Additional vendor-specific fields can be added here
        // Format: map.insert(0x8000 + vendor_specific_id, "vendor_field_name");
        
        map
    })
}

pub fn ipfix_field_name(field_type: u16) -> &'static str {
    ipfix_field_names().get(&field_type).copied().unwrap_or("unknown_field")
}

pub fn parse_ipfix_field(field: &TemplateField, data: &[u8], log_event: &mut LogEvent, max_field_length: usize) {
    use base64::Engine;
    
    // Check for enterprise-specific fields first
    if let Some(enterprise_id) = field.enterprise_number {
        // Handle HPE Aruba Networking EdgeConnect SD-WAN fields (enterprise 23867)
        if enterprise_id == 23867 {
            match field.field_type {
                // IPv4 Address fields (4 bytes)
                1 => { // clientIPv4Address
                    if data.len() >= 4 {
                        let addr = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("clientIPv4Address", format!("{}.{}.{}.{}",
                            (addr >> 24) & 0xFF, (addr >> 16) & 0xFF,
                            (addr >> 8) & 0xFF, addr & 0xFF));
                    }
                }
                2 => { // serverIPv4Address
                    if data.len() >= 4 {
                        let addr = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("serverIPv4Address", format!("{}.{}.{}.{}",
                            (addr >> 24) & 0xFF, (addr >> 16) & 0xFF,
                            (addr >> 8) & 0xFF, addr & 0xFF));
                    }
                }
                7 => { // connectionInitiator
                    if data.len() >= 4 {
                        let addr = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("connectionInitiator", format!("{}.{}.{}.{}",
                            (addr >> 24) & 0xFF, (addr >> 16) & 0xFF,
                            (addr >> 8) & 0xFF, addr & 0xFF));
                    }
                }
                
                // Unsigned 64-bit fields (8 bytes) - octets/packets
                3 => { // connectionServerOctetDeltaCount
                    if data.len() >= 8 {
                        let value = u64::from_be_bytes([data[0], data[1], data[2], data[3],
                                                      data[4], data[5], data[6], data[7]]);
                        log_event.insert("connectionServerOctetDeltaCount", value);
                    }
                }
                4 => { // connectionServerPacketDeltaCount
                    if data.len() >= 8 {
                        let value = u64::from_be_bytes([data[0], data[1], data[2], data[3],
                                                      data[4], data[5], data[6], data[7]]);
                        log_event.insert("connectionServerPacketDeltaCount", value);
                    }
                }
                5 => { // connectionClientOctetDeltaCount
                    if data.len() >= 8 {
                        let value = u64::from_be_bytes([data[0], data[1], data[2], data[3],
                                                      data[4], data[5], data[6], data[7]]);
                        log_event.insert("connectionClientOctetDeltaCount", value);
                    }
                }
                6 => { // connectionClientPacketDeltaCount
                    if data.len() >= 8 {
                        let value = u64::from_be_bytes([data[0], data[1], data[2], data[3],
                                                      data[4], data[5], data[6], data[7]]);
                        log_event.insert("connectionClientPacketDeltaCount", value);
                    }
                }
                
                // String fields (variable length)
                8 => { // applicationHttpHost
                    if !data.is_empty() {
                        match std::str::from_utf8(data) {
                            Ok(s) => {
                                let clean_str = s.trim_matches('\0');
                                log_event.insert("applicationHttpHost", clean_str);
                            }
                            Err(_) => {
                                let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                                log_event.insert("applicationHttpHost", encoded);
                            }
                        }
                    }
                }
                22 => { // fromZone
                    if !data.is_empty() {
                        match std::str::from_utf8(data) {
                            Ok(s) => {
                                let clean_str = s.trim_matches('\0');
                                log_event.insert("fromZone", clean_str);
                            }
                            Err(_) => {
                                let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                                log_event.insert("fromZone", encoded);
                            }
                        }
                    }
                }
                23 => { // toZone
                    if !data.is_empty() {
                        match std::str::from_utf8(data) {
                            Ok(s) => {
                                let clean_str = s.trim_matches('\0');
                                log_event.insert("toZone", clean_str);
                            }
                            Err(_) => {
                                let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                                log_event.insert("toZone", encoded);
                            }
                        }
                    }
                }
                24 => { // tag
                    if !data.is_empty() {
                        match std::str::from_utf8(data) {
                            Ok(s) => {
                                let clean_str = s.trim_matches('\0');
                                log_event.insert("tag", clean_str);
                            }
                            Err(_) => {
                                let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                                log_event.insert("tag", encoded);
                            }
                        }
                    }
                }
                25 => { // overlay
                    if !data.is_empty() {
                        match std::str::from_utf8(data) {
                            Ok(s) => {
                                let clean_str = s.trim_matches('\0');
                                log_event.insert("overlay", clean_str);
                            }
                            Err(_) => {
                                let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                                log_event.insert("overlay", encoded);
                            }
                        }
                    }
                }
                26 => { // direction
                    if !data.is_empty() {
                        match std::str::from_utf8(data) {
                            Ok(s) => {
                                let clean_str = s.trim_matches('\0');
                                log_event.insert("direction", clean_str);
                            }
                            Err(_) => {
                                let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                                log_event.insert("direction", encoded);
                            }
                        }
                    }
                }
                27 => { // applicationCategory
                    if !data.is_empty() {
                        match std::str::from_utf8(data) {
                            Ok(s) => {
                                let clean_str = s.trim_matches('\0');
                                log_event.insert("applicationCategory", clean_str);
                            }
                            Err(_) => {
                                let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                                log_event.insert("applicationCategory", encoded);
                            }
                        }
                    }
                }
                
                // Unsigned 8-bit fields (1 byte)
                9 => { // connectionNumberOfConnections
                    if data.len() >= 1 {
                        log_event.insert("connectionNumberOfConnections", data[0]);
                    }
                }
                10 => { // connectionServerResponsesCount
                    if data.len() >= 1 {
                        log_event.insert("connectionServerResponsesCount", data[0]);
                    }
                }
                21 => { // connectionTransactionCompleteCount
                    if data.len() >= 1 {
                        log_event.insert("connectionTransactionCompleteCount", data[0]);
                    }
                }
                
                // Unsigned 32-bit fields (4 bytes) - microseconds
                11 => { // connectionServerResponseDelay
                    if data.len() >= 4 {
                        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("connectionServerResponseDelay", value);
                    }
                }
                12 => { // connectionNetworkToServerDelay
                    if data.len() >= 4 {
                        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("connectionNetworkToServerDelay", value);
                    }
                }
                13 => { // connectionNetworkToClientDelay
                    if data.len() >= 4 {
                        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("connectionNetworkToClientDelay", value);
                    }
                }
                14 => { // connectionClientPacketRetransmissionCount
                    if data.len() >= 4 {
                        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("connectionClientPacketRetransmissionCount", value);
                    }
                }
                15 => { // connectionClientToServerNetworkDelay
                    if data.len() >= 4 {
                        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("connectionClientToServerNetworkDelay", value);
                    }
                }
                16 => { // connectionApplicationDelay
                    if data.len() >= 4 {
                        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("connectionApplicationDelay", value);
                    }
                }
                17 => { // connectionClientToServerResponseDelay
                    if data.len() >= 4 {
                        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("connectionClientToServerResponseDelay", value);
                    }
                }
                18 => { // connectionTransactionDuration
                    if data.len() >= 4 {
                        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("connectionTransactionDuration", value);
                    }
                }
                19 => { // connectionTransactionDurationMin
                    if data.len() >= 4 {
                        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("connectionTransactionDurationMin", value);
                    }
                }
                20 => { // connectionTransactionDurationMax
                    if data.len() >= 4 {
                        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("connectionTransactionDurationMax", value);
                    }
                }
                
                // Legacy fields (from previous documentation)
                10001 => { // overlayTunnelID - 4-byte tunnel identifier
                    if data.len() >= 4 {
                        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("overlayTunnelID", format!("0x{:08X}", value));
                    }
                }
                10002 => { // policyMatchID - 4-byte policy identifier
                    if data.len() >= 4 {
                        let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                        log_event.insert("policyMatchID", value);
                    }
                }
                10003 => { // applianceName - variable length string
                    if !data.is_empty() {
                        // Try to decode as UTF-8 string, fallback to hex if invalid
                        match std::str::from_utf8(data) {
                            Ok(s) => {
                                let clean_str = s.trim_matches('\0');
                                if clean_str.len() > max_field_length {
                                    log_event.insert("applianceName", format!("{}...", &clean_str[..max_field_length]));
                                } else {
                                    log_event.insert("applianceName", clean_str);
                                }
                            }
                            Err(_) => {
                                // Fallback to hex encoding for binary data
                                let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                                if encoded.len() > max_field_length {
                                    log_event.insert("applianceName", format!("{}...", &encoded[..max_field_length]));
                                } else {
                                    log_event.insert("applianceName", encoded);
                                }
                            }
                        }
                    }
                }
                10004 => { // WANInterfaceID - 2-byte interface identifier
                    if data.len() >= 2 {
                        let value = u16::from_be_bytes([data[0], data[1]]);
                        log_event.insert("WANInterfaceID", value);
                    }
                }
                10005 => { // QOSQueueID - 1-byte queue identifier
                    if data.len() >= 1 {
                        log_event.insert("QOSQueueID", data[0]);
                    }
                }
                10006 => { // linkQualityMetrics - variable length metrics data
                    if !data.is_empty() {
                        // Try to decode as JSON-like structure, fallback to hex
                        match std::str::from_utf8(data) {
                            Ok(s) => {
                                let clean_str = s.trim_matches('\0');
                                if clean_str.len() > max_field_length {
                                    log_event.insert("linkQualityMetrics", format!("{}...", &clean_str[..max_field_length]));
                                } else {
                                    log_event.insert("linkQualityMetrics", clean_str);
                                }
                            }
                            Err(_) => {
                                // Fallback to hex encoding for binary metrics data
                                let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                                if encoded.len() > max_field_length {
                                    log_event.insert("linkQualityMetrics", format!("{}...", &encoded[..max_field_length]));
                                } else {
                                    log_event.insert("linkQualityMetrics", encoded);
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Unknown HPE Aruba field - try to decode as string first
                    let ent_name = format!("hpe_aruba_field_{}", field.field_type);
                    match std::str::from_utf8(data) {
                        Ok(s) => {
                            let clean_str = s.trim_matches('\0');
                            if !clean_str.is_empty() && clean_str.chars().all(|c| c.is_ascii() && !c.is_control()) {
                                log_event.insert(ent_name.as_str(), clean_str);
                            } else {
                                let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                                if encoded.len() > max_field_length {
                                    log_event.insert(ent_name.as_str(), format!("{}...", &encoded[..max_field_length]));
                                } else {
                                    log_event.insert(ent_name.as_str(), encoded);
                                }
                            }
                        }
                        Err(_) => {
                            let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                            if encoded.len() > max_field_length {
                                log_event.insert(ent_name.as_str(), format!("{}...", &encoded[..max_field_length]));
                            } else {
                                log_event.insert(ent_name.as_str(), encoded);
                            }
                        }
                    }
                }
            }
        } else {
            // Other enterprise fields - try to decode as string first
            let ent_name = format!("enterprise_{}_{}", enterprise_id, field.field_type);
            match std::str::from_utf8(data) {
                Ok(s) => {
                    let clean_str = s.trim_matches('\0');
                    if !clean_str.is_empty() && clean_str.chars().all(|c| c.is_ascii() && !c.is_control()) {
                        log_event.insert(ent_name.as_str(), clean_str);
                    } else {
                        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                        if encoded.len() > max_field_length {
                            log_event.insert(ent_name.as_str(), format!("{}...", &encoded[..max_field_length]));
                        } else {
                            log_event.insert(ent_name.as_str(), encoded);
                        }
                    }
                }
                Err(_) => {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                    if encoded.len() > max_field_length {
                        log_event.insert(ent_name.as_str(), format!("{}...", &encoded[..max_field_length]));
                    } else {
                        log_event.insert(ent_name.as_str(), encoded);
                    }
                }
            }
        }
        return; // Exit early for enterprise fields
    }
    
    // Standard IPFIX fields
    let field_name = ipfix_field_name(field.field_type);
    match field.field_type {
        1 | 2 | 3 | 21 | 22 => { // 4-byte counters/timestamps
            if data.len() >= 4 {
                let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                log_event.insert(field_name, value);
            }
        }
        4 | 5 | 6 | 61 => { // 1-byte values
            if data.len() >= 1 {
                log_event.insert(field_name, data[0]);
            }
        }
        7 | 10 | 11 | 14 => { // 2-byte values
            if data.len() >= 2 {
                let value = u16::from_be_bytes([data[0], data[1]]);
                log_event.insert(field_name, value);
            }
        }
        8 | 12 | 15 | 18 => { // IPv4 addresses
            if data.len() >= 4 {
                let addr = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                log_event.insert(field_name, format!("{}.{}.{}.{}", (addr >> 24) & 0xFF, (addr >> 16) & 0xFF, (addr >> 8) & 0xFF, addr & 0xFF));
            }
        }
        27 | 28 => { // IPv6 addresses
            if data.len() >= 16 {
                log_event.insert(field_name, base64::engine::general_purpose::STANDARD.encode(data));
            }
        }
        _ => {
            // Standard fields - try to decode as string first
            match std::str::from_utf8(data) {
                Ok(s) => {
                    let clean_str = s.trim_matches('\0');
                    if !clean_str.is_empty() && clean_str.chars().all(|c| c.is_ascii() && !c.is_control()) {
                        log_event.insert(field_name, clean_str);
                    } else {
                        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                        if encoded.len() > max_field_length {
                            log_event.insert(field_name, format!("{}...", &encoded[..max_field_length]));
                        } else {
                            log_event.insert(field_name, encoded);
                        }
                    }
                }
                Err(_) => {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                    if encoded.len() > max_field_length {
                        log_event.insert(field_name, format!("{}...", &encoded[..max_field_length]));
                    } else {
                        log_event.insert(field_name, encoded);
                    }
                }
            }
        }
    }
} 

impl NetflowHeader {
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < NETFLOW_V5_HEADER_SIZE {
            return Err("Insufficient data for NetFlow v5 header");
        }
        
        Ok(NetflowHeader {
            version: u16::from_be_bytes([data[0], data[1]]),
            count: u16::from_be_bytes([data[2], data[3]]),
            sys_uptime: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            unix_secs: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            unix_nsecs: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
            flow_sequence: u32::from_be_bytes([data[16], data[17], data[18], data[19]]),
            engine_type: data[20],
            engine_id: data[21],
            sampling_interval: u16::from_be_bytes([data[22], data[23]]),
        })
    }
}

impl NetflowV5Record {
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 48 {
            return Err("Insufficient data for NetFlow v5 record");
        }
        
        Ok(NetflowV5Record {
            src_addr: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            dst_addr: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            nexthop: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            input: u16::from_be_bytes([data[12], data[13]]),
            output: u16::from_be_bytes([data[14], data[15]]),
            packets: u32::from_be_bytes([data[16], data[17], data[18], data[19]]),
            octets: u32::from_be_bytes([data[20], data[21], data[22], data[23]]),
            first: u32::from_be_bytes([data[24], data[25], data[26], data[27]]),
            last: u32::from_be_bytes([data[28], data[29], data[30], data[31]]),
            src_port: u16::from_be_bytes([data[32], data[33]]),
            dst_port: u16::from_be_bytes([data[34], data[35]]),
            pad1: data[36],
            tcp_flags: data[37],
            protocol: data[38],
            tos: data[39],
            src_as: u16::from_be_bytes([data[40], data[41]]),
            dst_as: u16::from_be_bytes([data[42], data[43]]),
            src_mask: data[44],
            dst_mask: data[45],
            pad2: u16::from_be_bytes([data[46], data[47]]),
        })
    }
}

impl NetflowV9Header {
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 20 {
            return Err("Insufficient data for NetFlow v9 header");
        }
        
        Ok(NetflowV9Header {
            version: u16::from_be_bytes([data[0], data[1]]),
            count: u16::from_be_bytes([data[2], data[3]]),
            sys_uptime: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            unix_secs: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            flow_sequence: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
            source_id: u32::from_be_bytes([data[16], data[17], data[18], data[19]]),
        })
    }
}

impl IpfixHeader {
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 16 {
            return Err("Insufficient data for IPFIX header");
        }
        
        Ok(IpfixHeader {
            version: u16::from_be_bytes([data[0], data[1]]),
            length: u16::from_be_bytes([data[2], data[3]]),
            export_time: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            sequence_number: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            observation_domain_id: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
        })
    }
}

impl SflowHeader {
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 28 {
            return Err("Insufficient data for sFlow header");
        }
        
        Ok(SflowHeader {
            version: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            agent_address_type: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            agent_address: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            sub_agent_id: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
            sequence_number: u32::from_be_bytes([data[16], data[17], data[18], data[19]]),
            sys_uptime: u32::from_be_bytes([data[20], data[21], data[22], data[23]]),
            num_samples: u32::from_be_bytes([data[24], data[25], data[26], data[27]]),
        })
    }
}

pub fn parse_netflow_v5(data: &[u8], peer_addr: SocketAddr, _template_cache: &TemplateCache, _max_field_length: usize) -> Result<Vec<Event>, &'static str> {
    if data.len() < NETFLOW_V5_HEADER_SIZE {
        return Err("Packet too short for NetFlow v5 header");
    }
    
    let header = NetflowHeader::from_bytes(&data[..NETFLOW_V5_HEADER_SIZE])?;
    let record_size = 48; // NetFlow v5 record size
    let expected_size = NETFLOW_V5_HEADER_SIZE + (header.count as usize * record_size);
    
    if data.len() < expected_size {
        return Err("Packet size doesn't match record count");
    }
    
    let mut events = Vec::new();
    let mut offset = NETFLOW_V5_HEADER_SIZE;
    
    for _ in 0..header.count {
        if offset + record_size > data.len() {
            break;
        }
        
        let record = NetflowV5Record::from_bytes(&data[offset..offset + record_size])?;
        let mut log_event = LogEvent::default();
        
        // Add standard NetFlow v5 fields
        log_event.insert("protocol", "netflow_v5");
        log_event.insert("peer_addr", peer_addr.to_string());
        log_event.insert("src_addr", format!("{}.{}.{}.{}", 
            (record.src_addr >> 24) & 0xFF, (record.src_addr >> 16) & 0xFF,
            (record.src_addr >> 8) & 0xFF, record.src_addr & 0xFF));
        log_event.insert("dst_addr", format!("{}.{}.{}.{}", 
            (record.dst_addr >> 24) & 0xFF, (record.dst_addr >> 16) & 0xFF,
            (record.dst_addr >> 8) & 0xFF, record.dst_addr & 0xFF));
        log_event.insert("src_port", record.src_port);
        log_event.insert("dst_port", record.dst_port);
        log_event.insert("protocol_id", record.protocol);
        log_event.insert("tos", record.tos);
        log_event.insert("tcp_flags", record.tcp_flags);
        log_event.insert("packets", record.packets);
        log_event.insert("octets", record.octets);
        log_event.insert("flow_start", record.first);
        log_event.insert("flow_end", record.last);
        log_event.insert("src_as", record.src_as);
        log_event.insert("dst_as", record.dst_as);
        log_event.insert("src_mask", record.src_mask);
        log_event.insert("dst_mask", record.dst_mask);
        log_event.insert("input", record.input);
        log_event.insert("output", record.output);
        log_event.insert("nexthop", format!("{}.{}.{}.{}", 
            (record.nexthop >> 24) & 0xFF, (record.nexthop >> 16) & 0xFF,
            (record.nexthop >> 8) & 0xFF, record.nexthop & 0xFF));
        
        events.push(Event::Log(log_event));
        offset += record_size;
    }
    
    Ok(events)
}

pub fn parse_netflow_v9(data: &[u8], peer_addr: SocketAddr, template_cache: &TemplateCache, max_field_length: usize) -> Result<Vec<Event>, &'static str> {
    if data.len() < 20 {
        return Err("Packet too short for NetFlow v9 header");
    }
    
    let _header = NetflowV9Header::from_bytes(&data[..20])?;
    let mut events = Vec::new();
    let mut offset = 20;
    
    while offset < data.len() {
        if offset + 4 > data.len() {
            break;
        }
        
        let set_id = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let set_length = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
        
        if set_length < 4 || offset + set_length > data.len() {
            break;
        }
        
        match set_id {
            0 => { // Template set
                let mut template_offset = offset + 4;
                while template_offset + 4 <= offset + set_length {
                    let template_id = u16::from_be_bytes([data[template_offset], data[template_offset + 1]]);
                    let field_count = u16::from_be_bytes([data[template_offset + 2], data[template_offset + 3]]) as usize;
                    
                    if template_offset + 4 + (field_count * 4) > offset + set_length {
                        break;
                    }
                    
                    let mut fields = Vec::new();
                    let mut field_offset = template_offset + 4;
                    
                    for _ in 0..field_count {
                        if field_offset + 4 > offset + set_length {
                            break;
                        }
                        
                        let field_type = u16::from_be_bytes([data[field_offset], data[field_offset + 1]]);
                        let field_length = u16::from_be_bytes([data[field_offset + 2], data[field_offset + 3]]);
                        
                        let enterprise_number = if field_type & 0x8000 != 0 {
                            if field_offset + 8 <= offset + set_length {
                                let ent_id = u32::from_be_bytes([
                                    data[field_offset + 4], data[field_offset + 5],
                                    data[field_offset + 6], data[field_offset + 7]
                                ]);
                                field_offset += 4;
                                Some(ent_id)
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        
                        fields.push(TemplateField {
                            field_type: field_type & 0x7FFF,
                            field_length,
                            enterprise_number,
                        });
                        
                        field_offset += 4;
                    }
                    
                    let template = Template {
                        template_id,
                        fields,
                        created: std::time::Instant::now(),
                    };
                    
                    let key = (peer_addr, 0, template_id); // Using 0 as observation domain for NetFlow v9
                    cache_put(template_cache, key, template);
                    
                    template_offset = field_offset;
                }
            }
            1 => { // Options template set
                // Similar to template set but for options
                // Implementation would be similar to template set
            }
            _ => { // Data set
                if let Some(template) = cache_get(template_cache, &(peer_addr, 0, set_id)) {
                    let mut data_offset = offset + 4;
                    while data_offset < offset + set_length {
                        let mut log_event = LogEvent::default();
                        log_event.insert("protocol", "netflow_v9");
                        log_event.insert("peer_addr", peer_addr.to_string());
                        log_event.insert("template_id", set_id);
                        
                        for field in &template.fields {
                            if data_offset + field.field_length as usize > offset + set_length {
                                break;
                            }
                            
                            let field_data = &data[data_offset..data_offset + field.field_length as usize];
                            parse_ipfix_field(field, field_data, &mut log_event, max_field_length);
                            data_offset += field.field_length as usize;
                        }
                        
                        events.push(Event::Log(log_event));
                    }
                }
            }
        }
        
        offset += set_length;
    }
    
    Ok(events)
}

pub fn parse_ipfix(data: &[u8], peer_addr: SocketAddr, template_cache: &TemplateCache, max_field_length: usize) -> Result<Vec<Event>, &'static str> {
    if data.len() < 16 {
        return Err("Packet too short for IPFIX header");
    }
    
    let header = IpfixHeader::from_bytes(&data[..16])?;
    let mut events = Vec::new();
    let mut offset = 16;
    
    while offset < data.len() {
        if offset + 4 > data.len() {
            break;
        }
        
        let set_id = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let set_length = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
        
        if set_length < 4 || offset + set_length > data.len() {
            break;
        }
        
        match set_id {
            2 => { // Template set
                let mut template_offset = offset + 4;
                while template_offset + 4 <= offset + set_length {
                    let template_id = u16::from_be_bytes([data[template_offset], data[template_offset + 1]]);
                    let field_count = u16::from_be_bytes([data[template_offset + 2], data[template_offset + 3]]) as usize;
                    
                    if template_offset + 4 + (field_count * 4) > offset + set_length {
                        break;
                    }
                    
                    let mut fields = Vec::new();
                    let mut field_offset = template_offset + 4;
                    
                    for _ in 0..field_count {
                        if field_offset + 4 > offset + set_length {
                            break;
                        }
                        
                        let field_type = u16::from_be_bytes([data[field_offset], data[field_offset + 1]]);
                        let field_length = u16::from_be_bytes([data[field_offset + 2], data[field_offset + 3]]);
                        
                        let enterprise_number = if field_type & 0x8000 != 0 {
                            if field_offset + 8 <= offset + set_length {
                                let ent_id = u32::from_be_bytes([
                                    data[field_offset + 4], data[field_offset + 5],
                                    data[field_offset + 6], data[field_offset + 7]
                                ]);
                                field_offset += 4;
                                Some(ent_id)
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        
                        fields.push(TemplateField {
                            field_type: field_type & 0x7FFF,
                            field_length,
                            enterprise_number,
                        });
                        
                        field_offset += 4;
                    }
                    
                    let template = Template {
                        template_id,
                        fields,
                        created: std::time::Instant::now(),
                    };
                    
                    let key = (peer_addr, header.observation_domain_id, template_id);
                    cache_put(template_cache, key, template);
                    
                    template_offset = field_offset;
                }
            }
            3 => { // Options template set
                // Similar to template set but for options
                // Implementation would be similar to template set
            }
            _ => { // Data set
                if let Some(template) = cache_get(template_cache, &(peer_addr, header.observation_domain_id, set_id)) {
                    let mut data_offset = offset + 4;
                    while data_offset < offset + set_length {
                        let mut log_event = LogEvent::default();
                        log_event.insert("protocol", "ipfix");
                        log_event.insert("peer_addr", peer_addr.to_string());
                        log_event.insert("template_id", set_id);
                        log_event.insert("observation_domain_id", header.observation_domain_id);
                        
                        for field in &template.fields {
                            if data_offset + field.field_length as usize > offset + set_length {
                                break;
                            }
                            
                            let field_data = &data[data_offset..data_offset + field.field_length as usize];
                            parse_ipfix_field(field, field_data, &mut log_event, max_field_length);
                            data_offset += field.field_length as usize;
                        }
                        
                        events.push(Event::Log(log_event));
                    }
                }
            }
        }
        
        offset += set_length;
    }
    
    Ok(events)
}

pub fn parse_sflow(data: &[u8], peer_addr: SocketAddr, _template_cache: &TemplateCache, _max_field_length: usize) -> Result<Vec<Event>, &'static str> {
    if data.len() < 28 {
        return Err("Packet too short for sFlow header");
    }
    
    let header = SflowHeader::from_bytes(&data[..28])?;
    let mut events = Vec::new();
    let mut offset = 28;
    
    for _ in 0..header.num_samples {
        if offset + 8 > data.len() {
            break;
        }
        
        let sample_type = u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]);
        let sample_length = u32::from_be_bytes([data[offset + 4], data[offset + 5], data[offset + 6], data[offset + 7]]) as usize;
        
        if offset + 8 + sample_length > data.len() {
            break;
        }
        
        let mut log_event = LogEvent::default();
        log_event.insert("protocol", "sflow");
        log_event.insert("peer_addr", peer_addr.to_string());
        log_event.insert("sample_type", sample_type);
        log_event.insert("agent_address", format!("{}.{}.{}.{}", 
            (header.agent_address >> 24) & 0xFF, (header.agent_address >> 16) & 0xFF,
            (header.agent_address >> 8) & 0xFF, header.agent_address & 0xFF));
        log_event.insert("sub_agent_id", header.sub_agent_id);
        log_event.insert("sequence_number", header.sequence_number);
        log_event.insert("sys_uptime", header.sys_uptime);
        
        // For now, just store the raw sample data
        if sample_length > 0 {
            let sample_data = &data[offset + 8..offset + 8 + sample_length];
            log_event.insert("sample_data", base64::engine::general_purpose::STANDARD.encode(sample_data));
        }
        
        events.push(Event::Log(log_event));
        offset += 8 + sample_length;
    }
    
    Ok(events)
}

pub fn parse_flow_data(data: &[u8], peer_addr: SocketAddr, template_cache: &TemplateCache, max_field_length: usize) -> Result<Vec<Event>, &'static str> {
    if data.len() < 2 {
        return Err("Packet too short to determine protocol");
    }
    
    let version = u16::from_be_bytes([data[0], data[1]]);
    
    match version {
        5 => parse_netflow_v5(data, peer_addr, template_cache, max_field_length),
        9 => parse_netflow_v9(data, peer_addr, template_cache, max_field_length),
        10 => parse_ipfix(data, peer_addr, template_cache, max_field_length),
        _ => {
            // Try sFlow (version 5)
            if data.len() >= 4 {
                let sflow_version = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                if sflow_version == 5 {
                    return parse_sflow(data, peer_addr, template_cache, max_field_length);
                }
            }
            Err("Unsupported flow protocol version")
        }
    }
} 