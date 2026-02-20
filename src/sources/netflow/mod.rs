//! NetFlow source for Vector.
//!
//! This source listens for NetFlow v5 packets over UDP and parses them into structured log events.

use std::sync::Arc;

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::time::Duration;
use tracing::{debug, error, info};

use crate::SourceSender;
use crate::config::{DataType, Resource, SourceConfig, SourceContext, SourceOutput};
use crate::shutdown::ShutdownSignal;
use crate::sources::Source;
use crate::sources::netflow::events::*;

/// Ensures ProtocolParser is Send + Sync so it can be shared across worker tasks.
#[allow(dead_code)]
fn assert_protocol_parser_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<protocols::ProtocolParser>();
}

pub mod config;
pub mod events;
pub mod fields;
pub mod protocols;
pub mod templates;

pub use config::NetflowConfig;

/// Creates a UDP socket. On Linux, enables SO_REUSEPORT for multi-worker load balancing; on other platforms, uses a single socket.
async fn create_bind_socket(address: std::net::SocketAddr) -> Result<UdpSocket, std::io::Error> {
    let domain = if address.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };

    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    #[cfg(target_os = "linux")]
    socket.set_reuse_port(true)?;
    socket.set_reuse_address(true)?;
    socket.bind(&address.into())?;
    socket.set_nonblocking(true)?;

    let std_socket: std::net::UdpSocket = socket.into();
    UdpSocket::from_std(std_socket)
}

/// NetFlow source with multi-socket support using SO_REUSEPORT.
async fn netflow_source(
    config: NetflowConfig,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> Result<(), ()> {
    #[cfg(target_os = "linux")]
    let num_workers = config.workers;
    #[cfg(not(target_os = "linux"))]
    let num_workers = 1;

    debug!(
        address = %config.address,
        num_workers = num_workers,
        message = "Starting NetFlow source with multi-socket support.",
    );

    // Create shared template cache and protocol parser
    let template_cache = Arc::new(templates::TemplateCache::new_with_buffering(
        config.max_templates,
        config.max_buffered_records,
    ));
    let protocol_parser = Arc::new(protocols::ProtocolParser::new(
        &config,
        (*template_cache).clone(),
    ));

    // Spawn worker tasks
    let mut worker_handles: Vec<tokio::task::JoinHandle<Result<(), ()>>> =
        Vec::with_capacity(num_workers);

    for worker_id in 0..num_workers {
        let socket = match create_bind_socket(config.address).await {
            Ok(s) => s,
            Err(error) => {
                emit!(NetflowBindError {
                    address: config.address,
                    error,
                });
                for handle in worker_handles.drain(..) {
                    handle.abort();
                }
                return Err(());
            }
        };

        let template_cache = template_cache.clone();
        let protocol_parser = protocol_parser.clone();
        let config = config.clone();
        let shutdown = shutdown.clone();
        let out = out.clone();

        let handle = tokio::spawn(async move {
            netflow_worker(
                worker_id,
                socket,
                config,
                template_cache,
                protocol_parser,
                out,
                shutdown,
            )
            .await
        });

        worker_handles.push(handle);
    }

    // Wait for all workers to complete
    for (worker_id, handle) in worker_handles.into_iter().enumerate() {
        if let Err(e) = handle.await {
            error!(
                worker_id = worker_id,
                error = %e,
                message = "NetFlow worker task failed.",
            );
            return Err(());
        }
    }

    info!(message = "All NetFlow workers completed.");
    Ok(())
}

/// Individual NetFlow worker task that processes packets from a single UDP socket.
///
/// Each worker runs in its own task and processes packets independently,
/// sharing the template cache and protocol parser with other workers.
async fn netflow_worker(
    worker_id: usize,
    socket: UdpSocket,
    config: NetflowConfig,
    template_cache: Arc<templates::TemplateCache>,
    protocol_parser: Arc<protocols::ProtocolParser>,
    mut out: SourceSender,
    mut shutdown: ShutdownSignal,
) -> Result<(), ()> {
    debug!(
        worker_id = worker_id,
        address = %config.address,
        message = "NetFlow worker started.",
    );

    // Buffer one byte larger than max to detect truncated UDP datagrams (OS truncates when recv buffer is full).
    let mut buffer = vec![0u8; config.max_packet_size + 1];
    let mut last_cleanup = std::time::Instant::now();

    loop {
        tokio::select! {
            recv_result = socket.recv_from(&mut buffer) => {
                match recv_result {
                    Ok((len, peer_addr)) => {
                        if len > config.max_packet_size {
                            emit!(NetflowParseError {
                                error: "Packet too large (truncated)",
                                protocol: "unknown",
                                peer_addr,
                            });
                            continue;
                        }

                        let data = &buffer[..len];
                        let events = protocol_parser.parse(data, peer_addr, &template_cache);

                        if !events.is_empty() {
                            emit!(NetflowEventsReceived {
                                count: events.len(),
                                byte_size: len,
                                peer_addr,
                            });

                            if let Err(error) = out.send_batch(events).await {
                                error!(
                                    worker_id = worker_id,
                                    message = "Error sending events.",
                                    %error,
                                );
                                return Err(());
                            }
                        }

                        // Periodic template cleanup (only one worker should do this)
                        if worker_id == 0 && last_cleanup.elapsed() > Duration::from_secs(300) {
                            template_cache.cleanup_expired(config.template_timeout);
                            last_cleanup = std::time::Instant::now();
                        }
                    }
                    Err(error) => {
                        emit!(NetflowReceiveError { error });
                    }
                }
            }
            _ = &mut shutdown => {
                debug!(
                    worker_id = worker_id,
                    message = "NetFlow worker shutting down.",
                );
                break;
            }
        }
    }

    debug!(worker_id = worker_id, message = "NetFlow worker completed.",);
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

    fn outputs(
        &self,
        _global_log_namespace: vector_lib::config::LogNamespace,
    ) -> Vec<SourceOutput> {
        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            vector_lib::schema::Definition::any(),
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
            workers: 1,
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
            strict_validation: true,
            include_raw_data: false,
        };

        let parser = ProtocolParser::new(&config, template_cache.clone());
        let peer_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 12345);

        // Create minimal NetFlow v5 packet
        let mut packet = vec![0u8; 72]; // 24 header + 48 record

        // NetFlow v5 header
        packet[0..2].copy_from_slice(&5u16.to_be_bytes()); // version
        packet[2..4].copy_from_slice(&1u16.to_be_bytes()); // count
        packet[4..8].copy_from_slice(&12345u32.to_be_bytes()); // sys_uptime
        packet[8..12].copy_from_slice(&1609459200u32.to_be_bytes()); // unix_secs
        packet[12..16].copy_from_slice(&0u32.to_be_bytes()); // unix_nsecs
        packet[16..20].copy_from_slice(&100u32.to_be_bytes()); // flow_sequence
        packet[20] = 0; // engine_type
        packet[21] = 0; // engine_id
        packet[22..24].copy_from_slice(&0u16.to_be_bytes()); // sampling_interval

        // Complete flow record (48 bytes)
        packet[24..28].copy_from_slice(&0xC0A80101u32.to_be_bytes()); // src_addr: 192.168.1.1
        packet[28..32].copy_from_slice(&0x0A000001u32.to_be_bytes()); // dst_addr: 10.0.0.1
        packet[32..36].copy_from_slice(&0u32.to_be_bytes()); // next_hop
        packet[36..38].copy_from_slice(&0u16.to_be_bytes()); // input
        packet[38..40].copy_from_slice(&0u16.to_be_bytes()); // output
        packet[40..44].copy_from_slice(&10u32.to_be_bytes()); // d_pkts
        packet[44..48].copy_from_slice(&1500u32.to_be_bytes()); // d_octets
        packet[48..52].copy_from_slice(&0u32.to_be_bytes()); // first
        packet[52..56].copy_from_slice(&0u32.to_be_bytes()); // last
        packet[56..58].copy_from_slice(&80u16.to_be_bytes()); // src_port
        packet[58..60].copy_from_slice(&443u16.to_be_bytes()); // dst_port
        packet[60] = 0; // pad1
        packet[61] = 0; // tcp_flags
        packet[62] = 6; // prot (TCP)
        packet[63] = 0; // tos
        packet[64..66].copy_from_slice(&0u16.to_be_bytes()); // src_as
        packet[66..68].copy_from_slice(&0u16.to_be_bytes()); // dst_as
        packet[68] = 24; // src_mask
        packet[69] = 24; // dst_mask
        packet[70..72].copy_from_slice(&0u16.to_be_bytes()); // pad2

        // Parse packet directly
        let events = parser.parse(&packet, peer_addr, &template_cache);

        assert!(!events.is_empty(), "No events received");

        let log = events[0].as_log();
        assert_eq!(
            log.get("flow_type").unwrap().as_str().unwrap(),
            "netflow_v5"
        );
        assert_eq!(
            log.get("src_addr").unwrap().as_str().unwrap(),
            "192.168.1.1"
        );
        assert_eq!(log.get("dst_addr").unwrap().as_str().unwrap(), "10.0.0.1");
        assert_eq!(log.get("protocol").unwrap().as_integer().unwrap(), 6);
        assert_eq!(log.get("src_port").unwrap().as_integer().unwrap(), 80);
        assert_eq!(log.get("dst_port").unwrap().as_integer().unwrap(), 443);
    }

    #[tokio::test]
    async fn test_invalid_packet_handling() {
        let addr = next_addr();
        let config = NetflowConfig {
            address: addr,
            workers: 1,
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
            strict_validation: true,
            include_raw_data: false,
        };

        let (tx, rx) = SourceSender::new_test();
        let cx = SourceContext::new_test(tx, None);
        let source = config.build(cx).await.unwrap();
        let _source_task = tokio::spawn(source);

        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(25)).await;
            let packet = vec![0u8; 5];
            if StdUdpSocket::bind("127.0.0.1:0")
                .and_then(|s| s.send_to(&packet, addr))
                .is_ok()
            {
                break;
            }
        }

        let events = tokio::time::timeout(Duration::from_millis(500), collect_ready(rx))
            .await
            .unwrap_or_default();

        if !events.is_empty() {
            if let Event::Log(log) = &events[0] {
                assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "unknown");
            }
        }
    }
}
