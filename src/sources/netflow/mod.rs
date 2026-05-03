//! Ingests NetFlow version 5 flow exports over UDP.
//!
//! Each datagram is decoded into structured log events.

use std::sync::Arc;

use socket2::{Domain, Protocol, SockRef, Socket, Type};
use tokio::net::UdpSocket;
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

use crate::config::{DataType, Resource, SourceConfig, SourceContext, SourceOutput};
use crate::shutdown::ShutdownSignal;
use crate::sources::netflow::events::*;
use crate::sources::Source;
use crate::SourceSender;

pub mod config;
pub mod events;
pub mod fields;
pub mod protocols;
pub mod templates;

pub use config::NetflowConfig;

fn reuseport_supported() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "freebsd"
    ))
}

/// Creates a UDP socket. On platforms with `SO_REUSEPORT` (Linux, macOS, FreeBSD), enables it for multi-worker load balancing.
async fn create_bind_socket(address: std::net::SocketAddr) -> Result<UdpSocket, std::io::Error> {
    let domain = if address.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };

    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
    socket.set_reuse_port(true)?;
    socket.set_reuse_address(true)?;

    let requested = 25 * 1024 * 1024usize;
    let _ = SockRef::from(&socket).set_recv_buffer_size(requested);
    let actual = SockRef::from(&socket).recv_buffer_size().unwrap_or(0);
    if actual < requested {
        warn!(
            message = "UDP receive buffer smaller than requested; packet loss possible under sustained load.",
            requested_bytes = requested,
            actual_bytes = actual,
            hint = "sudo sysctl -w net.core.rmem_max=26214400",
        );
    }

    socket.bind(&address.into())?;
    socket.set_nonblocking(true)?;

    let std_socket: std::net::UdpSocket = socket.into();
    UdpSocket::from_std(std_socket)
}

/// Runs the NetFlow listener with optional multi-socket fan-out (`SO_REUSEPORT` where supported).
async fn netflow_source(
    config: NetflowConfig,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> Result<(), ()> {
    let num_workers = if reuseport_supported() {
        config.workers
    } else if config.workers > 1 {
        warn!(
            message =
                "NetFlow `workers` > 1 requires SO_REUSEPORT; using one worker on this platform.",
            configured_workers = config.workers,
        );
        1
    } else {
        config.workers
    };

    debug!(
        address = %config.address,
        num_workers = num_workers,
        message = "Starting NetFlow source with multi-socket support.",
    );

    // Shared parser state (cache is a no-op for NetFlow v5).
    let template_cache = Arc::new(templates::TemplateCache::new(config.max_templates));
    let protocol_parser = Arc::new(protocols::ProtocolParser::new(
        &config,
        template_cache.as_ref().clone(),
    ));

    // Spawn worker tasks
    let mut worker_handles: Vec<Option<tokio::task::JoinHandle<Result<(), ()>>>> =
        Vec::with_capacity(num_workers);

    for worker_id in 0..num_workers {
        let socket = match create_bind_socket(config.address).await {
            Ok(s) => s,
            Err(error) => {
                emit!(NetflowBindError {
                    address: config.address,
                    error,
                });
                for slot in worker_handles.iter_mut() {
                    if let Some(handle) = slot.take() {
                        handle.abort();
                    }
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

        worker_handles.push(Some(handle));
    }

    // Wait for all workers to complete
    for (worker_id, slot) in worker_handles.iter_mut().enumerate() {
        let Some(handle) = slot.take() else {
            continue;
        };
        if let Err(e) = handle.await {
            error!(
                worker_id = worker_id,
                error = %e,
                message = "NetFlow worker task failed.",
            );
            for other in worker_handles.iter_mut().skip(worker_id + 1) {
                if let Some(h) = other.take() {
                    h.abort();
                }
            }
            return Err(());
        }
    }

    info!(message = "All NetFlow workers completed.");
    Ok(())
}

/// One UDP receive loop for an accepted worker slot.
///
/// Workers share the same [`templates::TemplateCache`] and [`protocols::ProtocolParser`] instances.
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

                        // Periodic cache sweep (worker 0 only; no-op for NetFlow v5).
                        let cleanup_period_secs = config.template_timeout.min(300);
                        if worker_id == 0
                            && last_cleanup.elapsed() > Duration::from_secs(cleanup_period_secs)
                        {
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
        if let Err(errors) = self.validate() {
            return Err(format!(
                "Invalid NetFlow source configuration: {}",
                errors.join("; ")
            )
            .into());
        }

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
            log.get("srcaddr").unwrap().as_str().unwrap(),
            "192.168.1.1"
        );
        assert_eq!(log.get("dstaddr").unwrap().as_str().unwrap(), "10.0.0.1");
        assert_eq!(log.get("prot").unwrap().as_integer().unwrap(), 6);
        assert_eq!(log.get("srcport").unwrap().as_integer().unwrap(), 80);
        assert_eq!(log.get("dstport").unwrap().as_integer().unwrap(), 443);
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
