use std::{
    io,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, UdpSocket as StdUdpSocket},
};

use listenfd::ListenFd;
use smallvec::SmallVec;
use tokio::net::UdpSocket;

#[cfg(target_os = "linux")]
use {
    nix::libc,
    std::{mem, os::fd::AsRawFd, ptr},
    tokio::io::unix::AsyncFd,
};

use super::SocketListenAddr;

pub const UDP_BATCH_SIZE: usize = 32;

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
}

impl<'a> UdpBatch<'a> {
    fn new(datagrams: SmallVec<[UdpDatagram<'a>; UDP_BATCH_SIZE]>) -> Self {
        Self { datagrams }
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
}

enum UdpBatchReceiverInner {
    #[cfg(target_os = "linux")]
    Linux(LinuxUdpBatchReceiver),
    #[cfg(not(target_os = "linux"))]
    Fallback(FallbackUdpBatchReceiver),
}

/// A UDP receiver that yields one or more datagrams at a time.
pub struct UdpBatchReceiver {
    inner: UdpBatchReceiverInner,
}

impl UdpBatchReceiver {
    pub fn from_socket(socket: UdpSocket, max_datagram_size: usize) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            return LinuxUdpBatchReceiver::from_socket(socket, max_datagram_size).map(|inner| {
                Self {
                    inner: UdpBatchReceiverInner::Linux(inner),
                }
            });
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
    last_address: SocketAddr,
    last_len: usize,
}

#[cfg(not(target_os = "linux"))]
impl FallbackUdpBatchReceiver {
    fn new(socket: UdpSocket, max_datagram_size: usize) -> Self {
        Self {
            socket,
            buffer: vec![0; max_datagram_size],
            last_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)),
            last_len: 0,
        }
    }

    async fn recv_batch(&mut self) -> io::Result<UdpBatch<'_>> {
        let (byte_size, address) = self.socket.recv_from(&mut self.buffer).await?;
        self.last_address = address;
        self.last_len = byte_size;

        let mut datagrams = SmallVec::new();
        datagrams.push(UdpDatagram {
            address: self.last_address,
            payload: &self.buffer[..self.last_len],
            truncated: self.last_len >= self.buffer.len(),
        });

        Ok(UdpBatch::new(datagrams))
    }
}

#[cfg(target_os = "linux")]
struct LinuxUdpBatchReceiver {
    socket: AsyncFd<StdUdpSocket>,
    addr_storage: Vec<libc::sockaddr_storage>,
    iovecs: Vec<libc::iovec>,
    headers: Vec<libc::mmsghdr>,
    buffers: Vec<Vec<u8>>,
}

#[cfg(target_os = "linux")]
unsafe impl Send for LinuxUdpBatchReceiver {}

#[cfg(target_os = "linux")]
impl LinuxUdpBatchReceiver {
    fn from_socket(socket: UdpSocket, max_datagram_size: usize) -> io::Result<Self> {
        let socket = socket.into_std()?;
        socket.set_nonblocking(true)?;

        let mut buffers = Vec::with_capacity(UDP_BATCH_SIZE);
        for _ in 0..UDP_BATCH_SIZE {
            buffers.push(vec![0; max_datagram_size]);
        }

        let mut receiver = Self {
            socket: AsyncFd::new(socket)?,
            addr_storage: (0..UDP_BATCH_SIZE)
                .map(|_| unsafe { mem::zeroed::<libc::sockaddr_storage>() })
                .collect(),
            iovecs: (0..UDP_BATCH_SIZE)
                .map(|_| unsafe { mem::zeroed::<libc::iovec>() })
                .collect(),
            headers: (0..UDP_BATCH_SIZE)
                .map(|_| unsafe { mem::zeroed::<libc::mmsghdr>() })
                .collect(),
            buffers,
        };
        receiver.reset_headers();
        Ok(receiver)
    }

    fn reset_headers(&mut self) {
        for index in 0..UDP_BATCH_SIZE {
            self.iovecs[index] = libc::iovec {
                iov_base: self.buffers[index].as_mut_ptr().cast(),
                iov_len: self.buffers[index].len(),
            };
            self.headers[index] = libc::mmsghdr {
                msg_hdr: libc::msghdr {
                    msg_name: (&mut self.addr_storage[index] as *mut libc::sockaddr_storage).cast(),
                    msg_namelen: mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t,
                    msg_iov: &mut self.iovecs[index] as *mut libc::iovec,
                    msg_iovlen: 1,
                    msg_control: ptr::null_mut(),
                    msg_controllen: 0,
                    msg_flags: 0,
                },
                msg_len: 0,
            };
        }
    }

    async fn recv_batch(&mut self) -> io::Result<UdpBatch<'_>> {
        let received = loop {
            let mut guard = self.socket.readable().await?;
            let addr_storage = &mut self.addr_storage;
            let iovecs = &mut self.iovecs;
            let headers = &mut self.headers;
            let buffers = &mut self.buffers;
            match guard.try_io(|socket| {
                try_recvmmsg(socket.get_ref(), addr_storage, iovecs, headers, buffers)
            }) {
                Ok(result) => break result?,
                Err(_would_block) => continue,
            }
        };

        let mut datagrams = SmallVec::with_capacity(received);
        for index in 0..received {
            let header = &self.headers[index];
            let msg_len = (header.msg_len as usize).min(self.buffers[index].len());
            let truncated = header.msg_len as usize >= self.buffers[index].len()
                || (header.msg_hdr.msg_flags & libc::MSG_TRUNC) != 0;

            datagrams.push(UdpDatagram {
                address: sockaddr_to_socket_addr(
                    &self.addr_storage[index],
                    header.msg_hdr.msg_namelen,
                )?,
                payload: &self.buffers[index][..msg_len],
                truncated,
            });
        }

        Ok(UdpBatch::new(datagrams))
    }
}

#[cfg(target_os = "linux")]
fn try_recvmmsg(
    socket: &StdUdpSocket,
    addr_storage: &mut [libc::sockaddr_storage],
    iovecs: &mut [libc::iovec],
    headers: &mut [libc::mmsghdr],
    buffers: &mut [Vec<u8>],
) -> io::Result<usize> {
    reset_headers(addr_storage, iovecs, headers, buffers);

    let received = unsafe {
        libc::recvmmsg(
            socket.as_raw_fd(),
            headers.as_mut_ptr(),
            headers.len() as u32,
            0,
            ptr::null_mut(),
        )
    };

    if received < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(received as usize)
    }
}

#[cfg(target_os = "linux")]
fn reset_headers(
    addr_storage: &mut [libc::sockaddr_storage],
    iovecs: &mut [libc::iovec],
    headers: &mut [libc::mmsghdr],
    buffers: &mut [Vec<u8>],
) {
    for index in 0..headers.len() {
        iovecs[index] = libc::iovec {
            iov_base: buffers[index].as_mut_ptr().cast(),
            iov_len: buffers[index].len(),
        };
        headers[index] = libc::mmsghdr {
            msg_hdr: libc::msghdr {
                msg_name: (&mut addr_storage[index] as *mut libc::sockaddr_storage).cast(),
                msg_namelen: mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t,
                msg_iov: &mut iovecs[index] as *mut libc::iovec,
                msg_iovlen: 1,
                msg_control: ptr::null_mut(),
                msg_controllen: 0,
                msg_flags: 0,
            },
            msg_len: 0,
        };
    }
}

#[cfg(target_os = "linux")]
fn sockaddr_to_socket_addr(
    storage: &libc::sockaddr_storage,
    len: libc::socklen_t,
) -> io::Result<SocketAddr> {
    match storage.ss_family as i32 {
        libc::AF_INET => {
            if len < mem::size_of::<libc::sockaddr_in>() as libc::socklen_t {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid IPv4 socket address length",
                ));
            }

            let addr = unsafe {
                ptr::read_unaligned(
                    (storage as *const libc::sockaddr_storage).cast::<libc::sockaddr_in>(),
                )
            };
            let ip = Ipv4Addr::from(u32::from_be(addr.sin_addr.s_addr));
            let port = u16::from_be(addr.sin_port);

            Ok(SocketAddr::V4(SocketAddrV4::new(ip, port)))
        }
        libc::AF_INET6 => {
            if len < mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid IPv6 socket address length",
                ));
            }

            let addr = unsafe {
                ptr::read_unaligned(
                    (storage as *const libc::sockaddr_storage).cast::<libc::sockaddr_in6>(),
                )
            };
            let ip = Ipv6Addr::from(addr.sin6_addr.s6_addr);
            let port = u16::from_be(addr.sin6_port);
            let flowinfo = u32::from_be(addr.sin6_flowinfo);

            Ok(SocketAddr::V6(SocketAddrV6::new(
                ip,
                port,
                flowinfo,
                addr.sin6_scope_id,
            )))
        }
        family => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported socket address family: {family}"),
        )),
    }
}

/// Binds a UDP socket to the listen address.
pub async fn try_bind_udp_socket(
    addr: SocketListenAddr,
    mut listenfd: ListenFd,
) -> io::Result<UdpSocket> {
    match addr {
        SocketListenAddr::SocketAddr(addr) => UdpSocket::bind(&addr).await,
        SocketListenAddr::SystemdFd(offset) => match listenfd.take_udp_socket(offset)? {
            Some(socket) => UdpSocket::from_std(socket),
            None => Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "systemd fd already consumed",
            )),
        },
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{collections::HashSet, net::SocketAddr, time::Duration};

    use tokio::{net::UdpSocket, time::timeout};

    use super::UdpBatchReceiver;

    #[tokio::test]
    async fn recvmmsg_receives_multiple_datagrams() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let mut receiver = UdpBatchReceiver::from_socket(socket, 64).unwrap();

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        for msg in ["one", "two", "three"] {
            sender.send_to(msg.as_bytes(), address).await.unwrap();
        }

        tokio::time::sleep(Duration::from_millis(20)).await;

        let batch = receiver.recv_batch().await.unwrap();
        assert_eq!(batch.len(), 3);
        let payloads = batch
            .iter()
            .map(|datagram| std::str::from_utf8(datagram.payload).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(payloads, vec!["one", "two", "three"]);
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

        let batch = receiver.recv_batch().await.unwrap();
        let peers = batch
            .iter()
            .map(|datagram| datagram.address)
            .collect::<HashSet<SocketAddr>>();
        let expected = HashSet::from([
            sender_one.local_addr().unwrap(),
            sender_two.local_addr().unwrap(),
        ]);

        assert_eq!(peers, expected);
    }

    #[tokio::test]
    async fn recvmmsg_reports_truncated_datagrams() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let mut receiver = UdpBatchReceiver::from_socket(socket, 4).unwrap();

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender.send_to(b"payload", address).await.unwrap();

        tokio::time::sleep(Duration::from_millis(20)).await;

        let batch = receiver.recv_batch().await.unwrap();
        let datagram = batch.iter().next().unwrap();
        assert!(datagram.truncated);
        assert_eq!(datagram.payload, b"payl");
    }

    #[tokio::test]
    async fn recvmmsg_waits_until_data_is_available() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let mut receiver = UdpBatchReceiver::from_socket(socket, 64).unwrap();

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let send_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            sender.send_to(b"hello", address).await.unwrap();
        });

        let batch = timeout(Duration::from_secs(1), receiver.recv_batch())
            .await
            .unwrap()
            .unwrap();

        send_task.await.unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.iter().next().unwrap().payload, b"hello");
    }
}
