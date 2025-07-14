use std::net::SocketAddr;
use std::str::FromStr;
use tokio::net::UdpSocket;
use vector_lib::event::{Event, LogEvent};
use vector_lib::test_util::collect_ready;

use crate::{
    config::{SourceConfig, SourceContext},
    sources::netflow::NetflowConfig,
    test_util::{start_topology, wait_for_tcp},
    SourceSender,
};

#[tokio::test]
async fn test_netflow_source_basic() {
    let config = NetflowConfig::from_address(
        SocketAddr::from_str("127.0.0.1:0").unwrap().into(),
    );

    let (tx, rx) = SourceSender::new_test();
    let context = SourceContext::new_test(tx, None);

    let source = config.build(context).await.unwrap();
    let mut topology = start_topology(source, None).await;

    // Wait for the source to start
    wait_for_tcp("127.0.0.1:0").await;

    // Send a simple NetFlow v5 packet (minimal valid packet)
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let target_addr = SocketAddr::from_str("127.0.0.1:0").unwrap();
    
    // Create a minimal NetFlow v5 packet
    let mut packet = Vec::new();
    
    // Version (5)
    packet.extend_from_slice(&5u16.to_be_bytes());
    // Count (1)
    packet.extend_from_slice(&1u16.to_be_bytes());
    // SysUptime (0)
    packet.extend_from_slice(&0u32.to_be_bytes());
    // UnixSecs (0)
    packet.extend_from_slice(&0u32.to_be_bytes());
    // UnixNsecs (0)
    packet.extend_from_slice(&0u32.to_be_bytes());
    // FlowSequence (0)
    packet.extend_from_slice(&0u32.to_be_bytes());
    // EngineType (0)
    packet.push(0);
    // EngineID (0)
    packet.push(0);
    // SamplingInterval (0)
    packet.extend_from_slice(&0u16.to_be_bytes());
    
    // Send the packet
    socket.send_to(&packet, target_addr).await.unwrap();

    // Wait a bit for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Check that we received an event
    let events = collect_ready(rx).await;
    assert!(!events.is_empty(), "Should have received at least one event");

    // Verify the event structure
    if let Event::Log(log) = &events[0] {
        assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "netflow_v5");
        assert_eq!(log.get("version").unwrap().as_u64().unwrap(), 5);
    }

    topology.stop().await;
}

#[tokio::test]
async fn test_netflow_source_multicast() {
    let mut config = NetflowConfig::from_address(
        SocketAddr::from_str("0.0.0.0:0").unwrap().into(),
    );
    config.multicast_groups = vec![
        std::net::Ipv4Addr::new(224, 0, 0, 2),
        std::net::Ipv4Addr::new(224, 0, 0, 4),
    ];

    let (tx, rx) = SourceSender::new_test();
    let context = SourceContext::new_test(tx, None);

    let source = config.build(context).await.unwrap();
    let mut topology = start_topology(source, None).await;

    // Wait for the source to start
    wait_for_tcp("0.0.0.0:0").await;

    // Send a multicast packet
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let target_addr = SocketAddr::from_str("224.0.0.2:0").unwrap();
    
    // Create a minimal NetFlow v5 packet
    let mut packet = Vec::new();
    packet.extend_from_slice(&5u16.to_be_bytes()); // Version
    packet.extend_from_slice(&1u16.to_be_bytes()); // Count
    packet.extend_from_slice(&0u32.to_be_bytes()); // SysUptime
    packet.extend_from_slice(&0u32.to_be_bytes()); // UnixSecs
    packet.extend_from_slice(&0u32.to_be_bytes()); // UnixNsecs
    packet.extend_from_slice(&0u32.to_be_bytes()); // FlowSequence
    packet.push(0); // EngineType
    packet.push(0); // EngineID
    packet.extend_from_slice(&0u16.to_be_bytes()); // SamplingInterval
    
    // Send the packet
    socket.send_to(&packet, target_addr).await.unwrap();

    // Wait a bit for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Check that we received an event
    let events = collect_ready(rx).await;
    assert!(!events.is_empty(), "Should have received at least one event");

    topology.stop().await;
}

#[tokio::test]
async fn test_netflow_source_unknown_protocol() {
    let config = NetflowConfig::from_address(
        SocketAddr::from_str("127.0.0.1:0").unwrap().into(),
    );

    let (tx, rx) = SourceSender::new_test();
    let context = SourceContext::new_test(tx, None);

    let source = config.build(context).await.unwrap();
    let mut topology = start_topology(source, None).await;

    // Wait for the source to start
    wait_for_tcp("127.0.0.1:0").await;

    // Send an unknown protocol packet
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let target_addr = SocketAddr::from_str("127.0.0.1:0").unwrap();
    
    // Create an unknown protocol packet
    let mut packet = Vec::new();
    packet.extend_from_slice(&99u16.to_be_bytes()); // Unknown version
    packet.extend_from_slice(&1u16.to_be_bytes()); // Count
    packet.extend_from_slice(&0u32.to_be_bytes()); // Some data
    
    // Send the packet
    socket.send_to(&packet, target_addr).await.unwrap();

    // Wait a bit for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Check that we received an event
    let events = collect_ready(rx).await;
    assert!(!events.is_empty(), "Should have received at least one event");

    // Verify the event structure
    if let Event::Log(log) = &events[0] {
        assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "unknown");
        assert_eq!(log.get("version").unwrap().as_u64().unwrap(), 99);
    }

    topology.stop().await;
} 