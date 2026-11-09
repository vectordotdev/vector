use std::{io, net::SocketAddr};

use listenfd::ListenFd;
use smallvec::SmallVec;
use tokio::net::UdpSocket;

#[cfg(target_os = "linux")]
use {
    nix::libc,
    std::{
        mem,
        net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6, UdpSocket as StdUdpSocket},
        os::fd::AsRawFd,
        ptr,
    },
    tokio::io::unix::AsyncFd,
};

use super::SocketListenAddr;

/// Maximum number of packets read by a single `recvmmsg` call.
pub(super) const UDP_BATCH_SIZE: usize = 32;

#[cfg(target_os = "linux")]
const UDP_RECEIVE_BUFFER_SIZE: usize = 65_536;
// Linux UAPI constants that libc does not expose on every architecture.
#[cfg(target_os = "linux")]
const UDP_GRO: libc::c_int = 104;
#[cfg(target_os = "linux")]
#[allow(clippy::cast_possible_truncation)]
const UDP_GRO_CONTROL_LEN: usize =
    // SAFETY: `CMSG_SPACE` only calculates a buffer size from the supplied value.
    unsafe { libc::CMSG_SPACE(mem::size_of::<libc::c_ushort>() as libc::c_uint) as usize };
#[cfg(target_os = "linux")]
const UDP_GRO_CONTROL_WORDS: usize = UDP_GRO_CONTROL_LEN.div_ceil(mem::size_of::<usize>());
#[cfg(target_os = "linux")]
type UdpGroControl = [usize; UDP_GRO_CONTROL_WORDS];

/// A received UDP datagram.
#[derive(Clone, Copy, Debug)]
pub struct UdpDatagram<'a> {
    pub address: SocketAddr,
    pub payload: &'a [u8],
    pub truncated: bool,
}

/// A batch of received UDP datagrams.
#[derive(Debug)]
pub struct UdpBatch<'a> {
    datagrams: SmallVec<[UdpDatagram<'a>; UDP_BATCH_SIZE]>,
    received_bytes: usize,
}

impl<'a> UdpBatch<'a> {
    const fn new(
        datagrams: SmallVec<[UdpDatagram<'a>; UDP_BATCH_SIZE]>,
        received_bytes: usize,
    ) -> Self {
        Self {
            datagrams,
            received_bytes,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.datagrams.is_empty()
    }

    pub fn len(&self) -> usize {
        self.datagrams.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &UdpDatagram<'a>> {
        self.datagrams.iter()
    }

    /// Total bytes observed on the socket before truncation or GRO splitting.
    pub const fn received_bytes(&self) -> usize {
        self.received_bytes
    }
}

enum UdpBatchReceiverInner {
    #[cfg(target_os = "linux")]
    Linux(LinuxUdpBatchReceiver),
    #[cfg(not(target_os = "linux"))]
    Fallback(FallbackUdpBatchReceiver),
}

/// A UDP receiver that yields one or more datagrams at a time.
///
/// Linux uses `recvmmsg` and enables UDP GRO. Other platforms retain the single-datagram Tokio
/// receive path.
pub struct UdpBatchReceiver {
    inner: UdpBatchReceiverInner,
}

impl UdpBatchReceiver {
    /// Creates a batched receiver from a Tokio UDP socket.
    ///
    /// # Errors
    ///
    /// Returns an error if the datagram size is zero or the platform receiver cannot be
    /// initialized.
    pub fn from_socket(socket: UdpSocket, max_datagram_size: usize) -> io::Result<Self> {
        if max_datagram_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "UDP datagram size must be greater than zero",
            ));
        }

        #[cfg(target_os = "linux")]
        {
            LinuxUdpBatchReceiver::from_socket(socket, max_datagram_size).map(|inner| Self {
                inner: UdpBatchReceiverInner::Linux(inner),
            })
        }

        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self {
                inner: UdpBatchReceiverInner::Fallback(FallbackUdpBatchReceiver::new(
                    socket,
                    max_datagram_size,
                )),
            })
        }
    }

    /// Receives the next nonempty batch of UDP datagrams.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying socket receive operation fails.
    pub async fn recv_batch(&mut self) -> io::Result<UdpBatch<'_>> {
        match &mut self.inner {
            #[cfg(target_os = "linux")]
            UdpBatchReceiverInner::Linux(inner) => inner.recv_batch().await,
            #[cfg(not(target_os = "linux"))]
            UdpBatchReceiverInner::Fallback(inner) => inner.recv_batch().await,
        }
    }
}

#[cfg(not(target_os = "linux"))]
struct FallbackUdpBatchReceiver {
    socket: UdpSocket,
    buffer: Vec<u8>,
    max_datagram_size: usize,
}

#[cfg(not(target_os = "linux"))]
impl FallbackUdpBatchReceiver {
    fn new(socket: UdpSocket, max_datagram_size: usize) -> Self {
        Self {
            socket,
            buffer: vec![0; max_datagram_size.saturating_add(1)],
            max_datagram_size,
        }
    }

    async fn recv_batch(&mut self) -> io::Result<UdpBatch<'_>> {
        let (byte_size, address) = self.socket.recv_from(&mut self.buffer).await?;
        let mut datagrams = SmallVec::new();
        datagrams.push(UdpDatagram {
            address,
            payload: &self.buffer[..byte_size.min(self.max_datagram_size)],
            truncated: byte_size > self.max_datagram_size,
        });
        Ok(UdpBatch::new(datagrams, byte_size))
    }
}

#[cfg(target_os = "linux")]
struct LinuxUdpBatchReceiver {
    socket: AsyncFd<StdUdpSocket>,
    max_datagram_size: usize,
    addresses: [libc::sockaddr_storage; UDP_BATCH_SIZE],
    controls: [UdpGroControl; UDP_BATCH_SIZE],
    buffers: Vec<Box<[u8]>>,
}

#[cfg(target_os = "linux")]
impl LinuxUdpBatchReceiver {
    fn from_socket(socket: UdpSocket, max_datagram_size: usize) -> io::Result<Self> {
        let socket = socket.into_std()?;
        socket.set_nonblocking(true)?;

        // UDP_GRO was added in Linux 5.0. If it is unavailable, recvmmsg batching still works.
        _ = enable_udp_gro(&socket);

        // SAFETY: `StdUdpSocket` owns a stable, valid descriptor for its entire lifetime.
        let socket = unsafe { AsyncFd::register(socket) }?;

        Ok(Self {
            socket,
            max_datagram_size,
            addresses: std::array::from_fn(|_| {
                // SAFETY: `sockaddr_storage` is an output buffer that the kernel initializes.
                unsafe { mem::zeroed() }
            }),
            controls: [[0; UDP_GRO_CONTROL_WORDS]; UDP_BATCH_SIZE],
            buffers: (0..UDP_BATCH_SIZE)
                .map(|_| vec![0; UDP_RECEIVE_BUFFER_SIZE].into_boxed_slice())
                .collect(),
        })
    }

    async fn recv_batch(&mut self) -> io::Result<UdpBatch<'_>> {
        let received = loop {
            let mut guard = self.socket.readable().await?;
            let addresses = &mut self.addresses;
            let controls = &mut self.controls;
            let buffers = &mut self.buffers;
            match guard
                .try_io(|socket| try_recvmmsg(socket.get_ref(), addresses, controls, buffers))
            {
                Ok(result) => break result?,
                Err(_would_block) => {}
            }
        };

        let mut datagrams = SmallVec::with_capacity(received.count);
        let mut received_bytes = 0usize;
        for message in 0..received.count {
            let header = &received.headers[message];
            received_bytes = received_bytes.saturating_add(header.msg_len as usize);
            let address =
                sockaddr_to_socket_addr(&self.addresses[message], header.msg_hdr.msg_namelen)?;
            let received_len = (header.msg_len as usize).min(self.buffers[message].len());
            let payload = &self.buffers[message][..received_len];
            let segment_size = udp_gro_segment_size(&header.msg_hdr).unwrap_or(received_len);
            let truncated = header.msg_len as usize > self.buffers[message].len()
                || (header.msg_hdr.msg_flags & libc::MSG_TRUNC) != 0;

            split_udp_payload(
                address,
                payload,
                segment_size,
                self.max_datagram_size,
                truncated,
                &mut datagrams,
            );
        }

        Ok(UdpBatch::new(datagrams, received_bytes))
    }
}

#[cfg(target_os = "linux")]
struct ReceivedMessages {
    headers: [libc::mmsghdr; UDP_BATCH_SIZE],
    count: usize,
}

#[cfg(target_os = "linux")]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn try_recvmmsg(
    socket: &StdUdpSocket,
    addresses: &mut [libc::sockaddr_storage; UDP_BATCH_SIZE],
    controls: &mut [UdpGroControl; UDP_BATCH_SIZE],
    buffers: &mut [Box<[u8]>],
) -> io::Result<ReceivedMessages> {
    let mut iovecs: [libc::iovec; UDP_BATCH_SIZE] = std::array::from_fn(|_| {
        // SAFETY: every field is initialized below before the kernel reads the structure.
        unsafe { mem::zeroed() }
    });
    let mut headers: [libc::mmsghdr; UDP_BATCH_SIZE] = std::array::from_fn(|_| {
        // SAFETY: every field read by `recvmmsg` is initialized below.
        unsafe { mem::zeroed() }
    });

    for index in 0..UDP_BATCH_SIZE {
        iovecs[index] = libc::iovec {
            iov_base: buffers[index].as_mut_ptr().cast(),
            iov_len: buffers[index].len(),
        };
        controls[index].fill(0);
        headers[index].msg_hdr = libc::msghdr {
            msg_name: (&raw mut addresses[index]).cast(),
            msg_namelen: mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t,
            msg_iov: &raw mut iovecs[index],
            msg_iovlen: 1,
            msg_control: controls[index].as_mut_ptr().cast(),
            msg_controllen: UDP_GRO_CONTROL_LEN,
            msg_flags: 0,
        };
    }

    // SAFETY: all pointers in `headers` refer to live, writable buffers above, and their
    // lengths remain valid for the duration of this nonblocking call.
    let received = unsafe {
        libc::recvmmsg(
            socket.as_raw_fd(),
            headers.as_mut_ptr(),
            UDP_BATCH_SIZE as libc::c_uint,
            libc::MSG_DONTWAIT | libc::MSG_TRUNC,
            ptr::null_mut(),
        )
    };

    if received < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ReceivedMessages {
            headers,
            count: received as usize,
        })
    }
}

#[cfg(target_os = "linux")]
#[allow(clippy::cast_possible_truncation)]
fn enable_udp_gro(socket: &StdUdpSocket) -> io::Result<()> {
    let enabled: libc::c_int = 1;
    // SAFETY: `enabled` is live for the call and its size matches the supplied option length.
    let result = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_UDP,
            UDP_GRO,
            (&raw const enabled).cast(),
            mem::size_of_val(&enabled) as libc::socklen_t,
        )
    };
    if result == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[allow(clippy::cast_possible_truncation)]
fn udp_gro_segment_size(header: &libc::msghdr) -> Option<usize> {
    // SAFETY: `header` comes from `recvmmsg`; its control pointer and length describe the
    // live ancillary-data buffer owned by the receiver.
    let mut control = unsafe { libc::CMSG_FIRSTHDR(header) };
    while !control.is_null() {
        // SAFETY: the CMSG traversal macros returned this non-null pointer within the buffer.
        let control_ref = unsafe { &*control };
        if control_ref.cmsg_level == libc::SOL_UDP
            && control_ref.cmsg_type == UDP_GRO
            && control_ref.cmsg_len
                >= unsafe {
                    // SAFETY: `CMSG_LEN` only calculates a length from the supplied value.
                    libc::CMSG_LEN(mem::size_of::<libc::c_ushort>() as libc::c_uint) as usize
                }
        {
            // SAFETY: the level, type, and length checks above establish that the control
            // payload contains a `c_ushort`; unaligned reads are permitted explicitly.
            let segment_size = unsafe {
                ptr::read_unaligned(libc::CMSG_DATA(control).cast::<libc::c_ushort>()) as usize
            };
            return (segment_size != 0).then_some(segment_size);
        }
        // SAFETY: `control` is the current element in the valid ancillary-data chain.
        control = unsafe { libc::CMSG_NXTHDR(header, control) };
    }
    None
}

#[cfg(target_os = "linux")]
fn split_udp_payload<'a>(
    address: SocketAddr,
    payload: &'a [u8],
    segment_size: usize,
    max_datagram_size: usize,
    receive_truncated: bool,
    datagrams: &mut SmallVec<[UdpDatagram<'a>; UDP_BATCH_SIZE]>,
) {
    if payload.is_empty() {
        datagrams.push(UdpDatagram {
            address,
            payload,
            truncated: receive_truncated,
        });
        return;
    }

    for segment in payload.chunks(segment_size.max(1)) {
        let payload_len = segment.len().min(max_datagram_size);
        datagrams.push(UdpDatagram {
            address,
            payload: &segment[..payload_len],
            truncated: receive_truncated || segment.len() > max_datagram_size,
        });
    }
}

#[cfg(target_os = "linux")]
#[allow(clippy::cast_lossless, clippy::cast_possible_truncation)]
fn sockaddr_to_socket_addr(
    storage: &libc::sockaddr_storage,
    len: libc::socklen_t,
) -> io::Result<SocketAddr> {
    match storage.ss_family as libc::c_int {
        libc::AF_INET => {
            if len < mem::size_of::<libc::sockaddr_in>() as libc::socklen_t {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid IPv4 socket address length",
                ));
            }
            // SAFETY: the family and checked length establish that `storage` contains a full
            // IPv4 socket address; unaligned reads are permitted explicitly.
            let address =
                unsafe { ptr::read_unaligned(ptr::from_ref(storage).cast::<libc::sockaddr_in>()) };
            Ok(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::from(u32::from_be(address.sin_addr.s_addr)),
                u16::from_be(address.sin_port),
            )))
        }
        libc::AF_INET6 => {
            if len < mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid IPv6 socket address length",
                ));
            }
            // SAFETY: the family and checked length establish that `storage` contains a full
            // IPv6 socket address; unaligned reads are permitted explicitly.
            let address =
                unsafe { ptr::read_unaligned(ptr::from_ref(storage).cast::<libc::sockaddr_in6>()) };
            Ok(SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::from(address.sin6_addr.s6_addr),
                u16::from_be(address.sin6_port),
                u32::from_be(address.sin6_flowinfo),
                address.sin6_scope_id,
            )))
        }
        family => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported socket address family: {family}"),
        )),
    }
}

/// Binds a UDP socket to the listen address.
///
/// # Errors
///
/// Returns an error if the address cannot be bound or the inherited socket cannot be configured.
pub async fn try_bind_udp_socket(
    addr: SocketListenAddr,
    mut listenfd: ListenFd,
) -> io::Result<UdpSocket> {
    match addr {
        SocketListenAddr::SocketAddr(addr) => UdpSocket::bind(&addr).await,
        SocketListenAddr::SystemdFd(offset) => match listenfd.take_udp_socket(offset)? {
            Some(socket) => {
                socket.set_nonblocking(true)?;
                UdpSocket::from_std(socket)
            }
            None => Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "systemd fd already consumed",
            )),
        },
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{collections::HashSet, mem, net::SocketAddr, ptr, time::Duration};

    use nix::libc;
    use smallvec::SmallVec;
    use tokio::{net::UdpSocket, time::timeout};

    use super::{
        UDP_BATCH_SIZE, UDP_GRO, UDP_GRO_CONTROL_LEN, UDP_GRO_CONTROL_WORDS, UdpBatchReceiver,
        UdpDatagram, split_udp_payload, udp_gro_segment_size,
    };

    #[tokio::test]
    async fn recvmmsg_receives_multiple_datagrams() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let mut receiver = UdpBatchReceiver::from_socket(socket, 64).unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        for message in ["one", "two", "three"] {
            sender.send_to(message.as_bytes(), address).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;

        let batch = receiver.recv_batch().await.unwrap();
        assert_eq!(batch.len(), 3);
        assert_eq!(batch.received_bytes(), 11);
        assert_eq!(
            batch.iter().map(|d| d.payload).collect::<Vec<_>>(),
            [b"one".as_slice(), b"two".as_slice(), b"three".as_slice()]
        );
    }

    #[tokio::test]
    async fn recvmmsg_preserves_peer_addresses() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let mut receiver = UdpBatchReceiver::from_socket(socket, 64).unwrap();
        let sender_one = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let sender_two = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender_one.send_to(b"one", address).await.unwrap();
        sender_two.send_to(b"two", address).await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;

        let peers = receiver
            .recv_batch()
            .await
            .unwrap()
            .iter()
            .map(|d| d.address)
            .collect::<HashSet<SocketAddr>>();
        assert_eq!(
            peers,
            HashSet::from([
                sender_one.local_addr().unwrap(),
                sender_two.local_addr().unwrap(),
            ])
        );
    }

    #[tokio::test]
    async fn recvmmsg_reports_truncated_datagrams() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let mut receiver = UdpBatchReceiver::from_socket(socket, 5).unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender.send_to(b"payload", address).await.unwrap();

        let batch = receiver.recv_batch().await.unwrap();
        let datagram = batch.iter().next().unwrap();
        assert!(datagram.truncated);
        assert_eq!(datagram.payload, b"paylo");
        assert_eq!(batch.received_bytes(), 7);
    }

    #[test]
    fn gro_segments_at_the_exact_limit_are_not_truncated() {
        let mut datagrams = SmallVec::new();
        split_udp_payload(
            "127.0.0.1:9000".parse().unwrap(),
            b"exact",
            5,
            5,
            false,
            &mut datagrams,
        );

        assert_eq!(datagrams.len(), 1);
        assert_eq!(datagrams[0].payload, b"exact");
        assert!(!datagrams[0].truncated);
    }

    #[tokio::test]
    async fn recvmmsg_waits_until_data_is_available() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let mut receiver = UdpBatchReceiver::from_socket(socket, 64).unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            sender.send_to(b"hello", address).await.unwrap();
        });

        let batch = timeout(Duration::from_secs(1), receiver.recv_batch())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(batch.iter().next().unwrap().payload, b"hello");
    }

    #[test]
    fn splits_gro_payload_into_datagrams() {
        let address = "127.0.0.1:9000".parse().unwrap();
        let mut datagrams: SmallVec<[UdpDatagram<'_>; UDP_BATCH_SIZE]> = SmallVec::new();
        split_udp_payload(address, b"aaaabbbbcc", 4, 64, false, &mut datagrams);
        assert_eq!(
            datagrams.iter().map(|d| d.payload).collect::<Vec<_>>(),
            [b"aaaa".as_slice(), b"bbbb".as_slice(), b"cc".as_slice()]
        );
        assert!(datagrams.iter().all(|d| !d.truncated));
        let batch = super::UdpBatch::new(datagrams, 10);
        assert_eq!(batch.received_bytes(), 10);
    }

    #[test]
    fn reads_gro_segment_size_control_message() {
        let mut control = [0usize; UDP_GRO_CONTROL_WORDS];
        // SAFETY: all fields consumed by the CMSG macros are initialized immediately below.
        let mut header: libc::msghdr = unsafe { mem::zeroed() };
        header.msg_control = control.as_mut_ptr().cast();
        header.msg_controllen = UDP_GRO_CONTROL_LEN;

        // SAFETY: `header` describes the live `control` buffer with its exact length.
        let control_header = unsafe { libc::CMSG_FIRSTHDR(&raw const header) };
        assert!(!control_header.is_null());
        // SAFETY: `control_header` points into `control`, and the buffer was sized with
        // `CMSG_SPACE` for the `c_ushort` written here.
        unsafe {
            (*control_header).cmsg_level = libc::SOL_UDP;
            (*control_header).cmsg_type = UDP_GRO;
            (*control_header).cmsg_len =
                libc::CMSG_LEN(mem::size_of::<libc::c_ushort>() as libc::c_uint) as usize;
            ptr::write_unaligned(
                libc::CMSG_DATA(control_header).cast::<libc::c_ushort>(),
                1_200,
            );
        }

        assert_eq!(udp_gro_segment_size(&header), Some(1_200));
    }
}
