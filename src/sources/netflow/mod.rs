//! NetFlow source for Vector.
//!
//! This source listens for NetFlow, IPFIX, and sFlow packets over UDP and parses them
//! into structured log events.

use crate::config::{DataType, Resource, SourceConfig, SourceContext, SourceOutput};
use crate::shutdown::ShutdownSignal;
use crate::sources::Source;
use crate::SourceSender;
use crate::sources::netflow::events::*;
use tokio::net::UdpSocket;
use tokio::time::Duration;
use vector_lib::internal_event::InternalEvent;

pub mod config;
pub mod events;
pub mod fields;
pub mod protocols;
pub mod templates;

pub use config::NetflowConfig;

/// Main netflow source implementation
async fn netflow_source(
    config: NetflowConfig,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let socket = UdpSocket::bind(config.address).await.map_err(|error| {
        NetflowBindError { 
            address: config.address,
            error,
        }.emit();
    })?;

    let template_cache = templates::TemplateCache::new_with_buffering(
        config.max_templates, 
        config.max_buffered_records
    );
    let protocol_parser = protocols::ProtocolParser::new(&config, template_cache.clone());
    
    // Pre-allocate multiple buffers for better performance
    let mut buffers = Vec::with_capacity(8);
    for _ in 0..8 {
        buffers.push(vec![0u8; config.max_packet_size]);
    }
    let mut buffer_index = 0;
    let mut last_cleanup = std::time::Instant::now();

    loop {
        tokio::select! {
            recv_result = socket.recv_from(&mut buffers[buffer_index]) => {
                match recv_result {
                    Ok((len, peer_addr)) => {
                        if len > config.max_packet_size {
                            NetflowParseError {
                                error: "Packet too large",
                                protocol: "unknown",
                                peer_addr,
                            }.emit();
                            continue;
                        }

                        let data = &buffers[buffer_index][..len];
                        let events = protocol_parser.parse(data, peer_addr, &template_cache);
                        
                        if !events.is_empty() {
                            NetflowEventsReceived {
                                count: events.len(),
                                byte_size: len,
                                peer_addr,
                            }.emit();

                            if let Err(error) = out.send_batch(events).await {
                                error!(message = "Error sending events", %error);
                                return Err(());
                            }
                        }

                        // Rotate buffer for next packet
                        buffer_index = (buffer_index + 1) % buffers.len();

                        // Periodic template cleanup
                        if last_cleanup.elapsed() > Duration::from_secs(300) {
                            template_cache.cleanup_expired(config.template_timeout);
                            last_cleanup = std::time::Instant::now();
                        }
                    }
                    Err(error) => {
                        NetflowReceiveError { error }.emit();
                        // Don't break on receive errors - keep trying
                    }
                }
            }
            _ = &mut shutdown => {
                info!("NetFlow source shutting down");
                break;
            }
        }
    }

    Ok(())
}

#[async_trait::async_trait]
#[typetag::serde(name = "netflow")]
impl SourceConfig for NetflowConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let config = self.clone();
        let shutdown = cx.shutdown;
        let out = cx.out;

        Ok(Box::pin(netflow_source(config, shutdown, out)))
    }

    fn outputs(&self, _global_log_namespace: vector_lib::config::LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_maybe_logs(
            DataType::Log, 
            vector_lib::schema::Definition::any()
        )]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::udp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{collect_ready, next_addr};
    use std::net::UdpSocket as StdUdpSocket;
    use vector_lib::event::Event;
    #[test]
    fn test_netflow_v5_parsing() {
        use crate::sources::netflow::protocols::ProtocolParser;
        use crate::sources::netflow::templates::TemplateCache;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let template_cache = TemplateCache::new(1000);
        let config = NetflowConfig {
            address: "127.0.0.1:2055".parse().unwrap(),
            max_packet_size: 1500,
            max_templates: 1000,
            template_timeout: 3600,
            protocols: vec!["netflow_v5".to_string()],
            parse_enterprise_fields: true,
            parse_options_templates: true,
            parse_variable_length_fields: true,
            enterprise_fields: std::collections::HashMap::new(),
            buffer_missing_templates: true,
            max_buffered_records: 100,
            options_template_mode: "emit_metadata".to_string(),
        };

        let parser = ProtocolParser::new(&config, template_cache.clone());
        let peer_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 12345);

        // Create minimal NetFlow v5 packet
        let mut packet = vec![0u8; 72]; // 24 header + 48 record
        
        // NetFlow v5 header
        packet[0..2].copy_from_slice(&5u16.to_be_bytes());     // version
        packet[2..4].copy_from_slice(&1u16.to_be_bytes());     // count
        packet[4..8].copy_from_slice(&12345u32.to_be_bytes()); // sys_uptime
        packet[8..12].copy_from_slice(&1609459200u32.to_be_bytes()); // unix_secs
        packet[12..16].copy_from_slice(&0u32.to_be_bytes());   // unix_nsecs
        packet[16..20].copy_from_slice(&100u32.to_be_bytes()); // flow_sequence
        packet[20] = 0; // engine_type
        packet[21] = 0; // engine_id
        packet[22..24].copy_from_slice(&0u16.to_be_bytes());   // sampling_interval
        
        // Complete flow record (48 bytes)
        packet[24..28].copy_from_slice(&0xC0A80101u32.to_be_bytes()); // src_addr: 192.168.1.1
        packet[28..32].copy_from_slice(&0x0A000001u32.to_be_bytes()); // dst_addr: 10.0.0.1
        packet[32..36].copy_from_slice(&0u32.to_be_bytes());   // next_hop
        packet[36..38].copy_from_slice(&0u16.to_be_bytes());   // input
        packet[38..40].copy_from_slice(&0u16.to_be_bytes());   // output
        packet[40..44].copy_from_slice(&10u32.to_be_bytes());  // d_pkts
        packet[44..48].copy_from_slice(&1500u32.to_be_bytes()); // d_octets
        packet[48..52].copy_from_slice(&0u32.to_be_bytes());   // first
        packet[52..56].copy_from_slice(&0u32.to_be_bytes());   // last
        packet[56..58].copy_from_slice(&80u16.to_be_bytes());  // src_port
        packet[58..60].copy_from_slice(&443u16.to_be_bytes()); // dst_port
        packet[60] = 0; // pad1
        packet[61] = 0; // tcp_flags
        packet[62] = 6; // prot (TCP)
        packet[63] = 0; // tos
        packet[64..66].copy_from_slice(&0u16.to_be_bytes());   // src_as
        packet[66..68].copy_from_slice(&0u16.to_be_bytes());   // dst_as
        packet[68] = 24; // src_mask
        packet[69] = 24; // dst_mask
        packet[70..72].copy_from_slice(&0u16.to_be_bytes());   // pad2

        // Parse packet directly
        let events = parser.parse(&packet, peer_addr, &template_cache);

        assert!(!events.is_empty(), "No events received");
        
        if let Event::Log(log) = &events[0] {
            // Debug: print all available fields
            if let Some(map) = log.as_map() {
                println!("Available fields: {:?}", map.keys().collect::<Vec<_>>());
            } else {
                println!("Log event is not a map");
            }
            
            // Check if flow_type exists first
            if let Some(flow_type) = log.get("flow_type") {
                if let Some(flow_type_str) = flow_type.as_str() {
                    assert_eq!(flow_type_str, "netflow_v5_record");
                } else {
                    panic!("flow_type is not a string: {:?}", flow_type);
                }
            } else {
                panic!("flow_type field not found in event");
            }
            
            // Check record fields (version is not included in record events)
            if let Some(src_addr) = log.get("src_addr") {
                if let Some(src_addr_str) = src_addr.as_str() {
                    assert_eq!(src_addr_str, "192.168.1.1");
                } else {
                    panic!("src_addr is not a string: {:?}", src_addr);
                }
            } else {
                panic!("src_addr field not found in event");
            }
            
            if let Some(dst_addr) = log.get("dst_addr") {
                if let Some(dst_addr_str) = dst_addr.as_str() {
                    assert_eq!(dst_addr_str, "10.0.0.1");
                } else {
                    panic!("dst_addr is not a string: {:?}", dst_addr);
                }
            } else {
                panic!("dst_addr field not found in event");
            }
            
            // Check other important fields
            if let Some(protocol) = log.get("protocol") {
                if let Some(protocol_int) = protocol.as_integer() {
                    assert_eq!(protocol_int, 6); // TCP
                } else {
                    panic!("protocol is not an integer: {:?}", protocol);
                }
            } else {
                panic!("protocol field not found in event");
            }
            
            if let Some(src_port) = log.get("src_port") {
                if let Some(src_port_int) = src_port.as_integer() {
                    assert_eq!(src_port_int, 80);
                } else {
                    panic!("src_port is not an integer: {:?}", src_port);
                }
            } else {
                panic!("src_port field not found in event");
            }
            
            if let Some(dst_port) = log.get("dst_port") {
                if let Some(dst_port_int) = dst_port.as_integer() {
                    assert_eq!(dst_port_int, 443);
                } else {
                    panic!("dst_port is not an integer: {:?}", dst_port);
                }
            } else {
                panic!("dst_port field not found in event");
            }
        } else {
            panic!("Expected Log event, got {:?}", events[0]);
        }
    }

    #[tokio::test]
    async fn test_invalid_packet_handling() {
        let addr = next_addr();
        let config = NetflowConfig {
            address: addr,
            max_packet_size: 1500,
            max_templates: 1000,
            template_timeout: 3600,
            protocols: vec!["netflow_v5".to_string()],
            parse_enterprise_fields: true,
            parse_options_templates: true,
            parse_variable_length_fields: true,
            enterprise_fields: std::collections::HashMap::new(),
            buffer_missing_templates: true,
            max_buffered_records: 100,
            options_template_mode: "emit_metadata".to_string(),
        };

        let (tx, rx) = SourceSender::new_test();
        let cx = SourceContext::new_test(tx, None);
        let source = config.build(cx).await.unwrap();
        let _source_task = tokio::spawn(source);
        
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Send invalid packet (too short)
        let packet = vec![0u8; 5];
        let socket = StdUdpSocket::bind("127.0.0.1:0").unwrap();
        socket.send_to(&packet, addr).unwrap();

        // Should either get no events or an unknown protocol event
        let events = tokio::time::timeout(Duration::from_millis(500), collect_ready(rx))
            .await
            .unwrap_or_default();

        // Invalid packets might create unknown protocol events or be dropped
        // Both behaviors are acceptable
        if !events.is_empty() {
            if let Event::Log(log) = &events[0] {
                assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "unknown");
            }
        }
    }
}
