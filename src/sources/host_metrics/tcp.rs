use crate::sources::host_metrics::HostMetricsScrapeDetailError;
use byteorder::{ByteOrder, NativeEndian};
use std::{collections::HashMap, io, path::Path};
use vector_lib::event::MetricTags;

use netlink_packet_core::{
    NetlinkHeader, NetlinkMessage, NetlinkPayload, NLM_F_ACK, NLM_F_DUMP, NLM_F_REQUEST,
};
use netlink_packet_sock_diag::{
    constants::*,
    inet::{ExtensionFlags, InetRequest, InetResponseHeader, SocketId, StateFlags},
    SockDiagMessage,
};
use netlink_sys::{
    protocols::NETLINK_SOCK_DIAG, AsyncSocket, AsyncSocketExt, SocketAddr, TokioSocket,
};
use snafu::{ResultExt, Snafu};

use super::HostMetrics;

const PROC_IPV6_FILE: &str = "/proc/net/if_inet6";
const TCP_CONNS_TOTAL: &str = "tcp_connections_total";
const TCP_TX_QUEUED_BYTES_TOTAL: &str = "tcp_tx_queued_bytes_total";
const TCP_RX_QUEUED_BYTES_TOTAL: &str = "tcp_rx_queued_bytes_total";
const STATE: &str = "state";

impl HostMetrics {
    pub async fn tcp_metrics(&self, output: &mut super::MetricsBuffer) {
        match build_tcp_stats().await {
            Ok(stats) => {
                output.name = "tcp";
                for (state, count) in stats.conn_states {
                    let tags = metric_tags! {
                        STATE => state
                    };
                    output.gauge(TCP_CONNS_TOTAL, count, tags);
                }

                output.gauge(
                    TCP_TX_QUEUED_BYTES_TOTAL,
                    stats.tx_queued_bytes,
                    MetricTags::default(),
                );
                output.gauge(
                    TCP_RX_QUEUED_BYTES_TOTAL,
                    stats.rx_queued_bytes,
                    MetricTags::default(),
                );
            }
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load tcp connection info.",
                    error,
                });
            }
        }
    }
}

#[derive(Debug, Snafu)]
enum TcpError {
    #[snafu(display("Could not open new netlink socket: {}", source))]
    NetlinkSocket { source: io::Error },
    #[snafu(display("Could not send netlink message: {}", source))]
    NetlinkSend { source: io::Error },
    #[snafu(display("Could not parse netlink response: {}", source))]
    NetlinkParse {
        source: netlink_packet_utils::DecodeError,
    },
    #[snafu(display("Could not recognize TCP state {state}"))]
    InvalidTcpState { state: u8 },
    #[snafu(display("Received an error message from netlink; code: {code}"))]
    NetlinkMsgError { code: i32 },
}

#[repr(u8)]
enum TcpState {
    Established = 1,
    SynSent = 2,
    SynRecv = 3,
    FinWait1 = 4,
    FinWait2 = 5,
    TimeWait = 6,
    Close = 7,
    CloseWait = 8,
    LastAck = 9,
    Listen = 10,
    Closing = 11,
}

impl From<TcpState> for String {
    fn from(val: TcpState) -> Self {
        match val {
            TcpState::Established => "established".into(),
            TcpState::SynSent => "syn_sent".into(),
            TcpState::SynRecv => "syn_recv".into(),
            TcpState::FinWait1 => "fin_wait1".into(),
            TcpState::FinWait2 => "fin_wait2".into(),
            TcpState::TimeWait => "time_wait".into(),
            TcpState::Close => "close".into(),
            TcpState::CloseWait => "close_wait".into(),
            TcpState::LastAck => "last_ack".into(),
            TcpState::Listen => "listen".into(),
            TcpState::Closing => "closing".into(),
        }
    }
}

impl TryFrom<u8> for TcpState {
    type Error = TcpError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(TcpState::Established),
            2 => Ok(TcpState::SynSent),
            3 => Ok(TcpState::SynRecv),
            4 => Ok(TcpState::FinWait1),
            5 => Ok(TcpState::FinWait2),
            6 => Ok(TcpState::TimeWait),
            7 => Ok(TcpState::Close),
            8 => Ok(TcpState::CloseWait),
            9 => Ok(TcpState::LastAck),
            10 => Ok(TcpState::Listen),
            11 => Ok(TcpState::Closing),
            _ => Err(TcpError::InvalidTcpState { state: value }),
        }
    }
}

#[derive(Debug, Default)]
struct TcpStats {
    conn_states: HashMap<String, f64>,
    rx_queued_bytes: f64,
    tx_queued_bytes: f64,
}

async fn fetch_nl_inet_hdrs(addr_family: u8) -> Result<Vec<InetResponseHeader>, TcpError> {
    let unicast_socket: SocketAddr = SocketAddr::new(0, 0);
    let mut socket = TokioSocket::new(NETLINK_SOCK_DIAG).context(NetlinkSocketSnafu)?;

    let mut inet_req = InetRequest {
        family: addr_family,
        protocol: IPPROTO_TCP,
        extensions: ExtensionFlags::INFO,
        states: StateFlags::all(),
        socket_id: SocketId::new_v4(),
    };
    if addr_family == AF_INET6 {
        inet_req.socket_id = SocketId::new_v6();
    }

    let mut hdr = NetlinkHeader::default();
    hdr.flags = NLM_F_REQUEST | NLM_F_ACK | NLM_F_DUMP;
    let mut msg = NetlinkMessage::new(hdr, SockDiagMessage::InetRequest(inet_req).into());
    msg.finalize();

    let mut buf = vec![0; msg.header.length as usize];
    msg.serialize(&mut buf[..]);

    socket
        .send_to(&buf[..msg.buffer_len()], &unicast_socket)
        .await
        .context(NetlinkSendSnafu)?;

    let mut receive_buffer = vec![0; 4096];
    let mut inet_resp_hdrs: Vec<InetResponseHeader> = Vec::new();
    'outer: while let Ok(()) = socket.recv(&mut &mut receive_buffer[..]).await {
        let mut offset = 0;
        'inner: loop {
            let bytes = &receive_buffer[offset..];
            let length = NativeEndian::read_u32(&bytes[0..4]) as usize;
            if length == 0 {
                break 'inner;
            }
            let rx_packet =
                <NetlinkMessage<SockDiagMessage>>::deserialize(bytes).context(NetlinkParseSnafu)?;

            match rx_packet.payload {
                NetlinkPayload::InnerMessage(SockDiagMessage::InetResponse(response)) => {
                    inet_resp_hdrs.push(response.header);
                }
                NetlinkPayload::Done(_) => {
                    break 'outer;
                }
                NetlinkPayload::Error(error) => {
                    if let Some(code) = error.code {
                        return Err(TcpError::NetlinkMsgError { code: code.get() });
                    }
                }
                _ => {}
            }

            offset += rx_packet.header.length as usize;
        }
    }

    Ok(inet_resp_hdrs)
}

fn parse_nl_inet_hdrs(
    hdrs: Vec<InetResponseHeader>,
    tcp_stats: &mut TcpStats,
) -> Result<(), TcpError> {
    for hdr in hdrs {
        let state: TcpState = hdr.state.try_into()?;
        let state_str: String = state.into();
        *tcp_stats.conn_states.entry(state_str).or_insert(0.0) += 1.0;
        tcp_stats.tx_queued_bytes += f64::from(hdr.send_queue);
        tcp_stats.rx_queued_bytes += f64::from(hdr.recv_queue)
    }

    Ok(())
}

async fn build_tcp_stats() -> Result<TcpStats, TcpError> {
    let mut tcp_stats = TcpStats::default();
    let resp = fetch_nl_inet_hdrs(AF_INET).await?;
    parse_nl_inet_hdrs(resp, &mut tcp_stats)?;

    if is_ipv6_enabled() {
        let resp = fetch_nl_inet_hdrs(AF_INET6).await?;
        parse_nl_inet_hdrs(resp, &mut tcp_stats)?;
    }

    Ok(tcp_stats)
}

fn is_ipv6_enabled() -> bool {
    Path::new(PROC_IPV6_FILE).exists()
}

#[cfg(test)]
mod tests {
    use tokio::net::{TcpListener, TcpStream};

    use netlink_packet_sock_diag::{
        inet::{InetResponseHeader, SocketId},
        AF_INET,
    };

    use crate::sources::host_metrics::{HostMetrics, HostMetricsConfig, MetricsBuffer};

    use super::{
        fetch_nl_inet_hdrs, parse_nl_inet_hdrs, TcpStats, STATE, TCP_CONNS_TOTAL,
        TCP_RX_QUEUED_BYTES_TOTAL, TCP_TX_QUEUED_BYTES_TOTAL,
    };

    #[test]
    fn parses_nl_inet_hdrs() {
        let mut hdrs: Vec<InetResponseHeader> = Vec::new();
        for i in 1..4 {
            let hdr = InetResponseHeader {
                family: 0,
                state: i,
                timer: None,
                socket_id: SocketId::new_v4(),
                recv_queue: 3,
                send_queue: 5,
                uid: 0,
                inode: 0,
            };
            hdrs.push(hdr);
        }

        let mut tcp_stats = TcpStats::default();
        parse_nl_inet_hdrs(hdrs, &mut tcp_stats).unwrap();

        assert_eq!(tcp_stats.tx_queued_bytes, 15.0);
        assert_eq!(tcp_stats.rx_queued_bytes, 9.0);
        assert_eq!(tcp_stats.conn_states.len(), 3);
        assert_eq!(*tcp_stats.conn_states.get("established").unwrap(), 1.0);
        assert_eq!(*tcp_stats.conn_states.get("syn_sent").unwrap(), 1.0);
        assert_eq!(*tcp_stats.conn_states.get("syn_recv").unwrap(), 1.0);
    }

    #[tokio::test]
    async fn fetches_nl_net_hdrs() {
        // start a TCP server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            // accept a connection
            let (_stream, _socket) = listener.accept().await.unwrap();
        });
        // initiate a connection
        let _stream = TcpStream::connect(addr).await.unwrap();

        let hdrs = fetch_nl_inet_hdrs(AF_INET).await.unwrap();
        // there should be at least two connections, one for the server and one for the client
        assert!(hdrs.len() >= 2);

        // assert that we have one connection with the server's port as the source port and
        // one as the destination port
        let mut source = false;
        let mut dst = false;
        for hdr in hdrs {
            if hdr.socket_id.source_port == addr.port() {
                source = true;
            }
            if hdr.socket_id.destination_port == addr.port() {
                dst = true;
            }
        }
        assert!(source);
        assert!(dst);
    }

    #[tokio::test]
    async fn generates_tcp_metrics() {
        let _listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .tcp_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;

        assert!(!metrics.is_empty());

        let mut n_tx_queued_bytes_metric = 0;
        let mut n_rx_queued_bytes_metric = 0;

        for metric in metrics {
            if metric.name() == TCP_CONNS_TOTAL {
                let tags = metric.tags();
                assert!(
                    tags.is_some(),
                    "Metric tcp_connections_total must have a tag"
                );
                let tags = tags.unwrap();
                assert!(
                    tags.contains_key(STATE),
                    "Metric tcp_connections_total must have a mode tag"
                );
            } else if metric.name() == TCP_TX_QUEUED_BYTES_TOTAL {
                n_tx_queued_bytes_metric += 1;
            } else if metric.name() == TCP_RX_QUEUED_BYTES_TOTAL {
                n_rx_queued_bytes_metric += 1;
            } else {
                panic!("unrecognized metric name");
            }
        }

        assert_eq!(n_tx_queued_bytes_metric, 1);
        assert_eq!(n_rx_queued_bytes_metric, 1);
    }
}
