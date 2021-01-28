#[cfg(feature = "listenfd")]
use super::Handshake;
use super::{
    CreateAcceptor, IncomingListener, MaybeTlsSettings, MaybeTlsStream, TcpBind, TlsError,
    TlsSettings,
};
#[cfg(feature = "sources-utils-tcp-keepalive")]
use crate::tcp::TcpKeepaliveConfig;
use bytes::{Buf, BufMut};
use futures::{future::BoxFuture, stream, FutureExt, Stream};
use openssl::ssl::{SslAcceptor, SslMethod};
use snafu::ResultExt;
use std::{
    future::Future,
    mem::MaybeUninit,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream},
};
use tokio_openssl::{HandshakeError, SslStream};

impl TlsSettings {
    pub(crate) fn acceptor(&self) -> crate::tls::Result<SslAcceptor> {
        match self.identity {
            None => Err(TlsError::MissingRequiredIdentity),
            Some(_) => {
                let mut acceptor =
                    SslAcceptor::mozilla_intermediate(SslMethod::tls()).context(CreateAcceptor)?;
                self.apply_context(&mut acceptor)?;
                Ok(acceptor.build())
            }
        }
    }
}

impl MaybeTlsSettings {
    pub(crate) async fn bind(&self, addr: &SocketAddr) -> crate::tls::Result<MaybeTlsListener> {
        let listener = TcpListener::bind(addr).await.context(TcpBind)?;

        let acceptor = match self {
            Self::Tls(tls) => Some(tls.acceptor()?),
            Self::Raw(()) => None,
        };

        Ok(MaybeTlsListener { listener, acceptor })
    }
}

pub(crate) struct MaybeTlsListener {
    listener: TcpListener,
    acceptor: Option<SslAcceptor>,
}

impl MaybeTlsListener {
    pub(crate) async fn accept(&mut self) -> crate::tls::Result<MaybeTlsIncomingStream<TcpStream>> {
        self.listener
            .accept()
            .await
            .map(|(stream, peer_addr)| {
                MaybeTlsIncomingStream::new(stream, peer_addr, self.acceptor.clone())
            })
            .context(IncomingListener)
    }

    async fn into_accept(
        mut self,
    ) -> (crate::tls::Result<MaybeTlsIncomingStream<TcpStream>>, Self) {
        (self.accept().await, self)
    }

    pub(crate) fn accept_stream(
        self,
    ) -> impl Stream<Item = crate::tls::Result<MaybeTlsIncomingStream<TcpStream>>> {
        let mut accept = Box::pin(self.into_accept());
        stream::poll_fn(move |context| match accept.as_mut().poll(context) {
            Poll::Ready((item, this)) => {
                accept.set(this.into_accept());
                Poll::Ready(Some(item))
            }
            Poll::Pending => Poll::Pending,
        })
    }

    #[cfg(feature = "listenfd")]
    pub(crate) fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.listener.local_addr()
    }
}

impl From<TcpListener> for MaybeTlsListener {
    fn from(listener: TcpListener) -> Self {
        Self {
            listener,
            acceptor: None,
        }
    }
}

pub struct MaybeTlsIncomingStream<S> {
    state: StreamState<S>,
    // BoxFuture doesn't allow access to the inner stream, but users
    // of MaybeTlsIncomingStream want access to the peer address while
    // still handshaking, so we have to cache it here.
    peer_addr: SocketAddr,
}

enum StreamState<S> {
    Accepted(MaybeTlsStream<S>),
    Accepting(BoxFuture<'static, Result<SslStream<S>, HandshakeError<S>>>),
    AcceptError(String),
}

impl<S> MaybeTlsIncomingStream<S> {
    #[cfg_attr(not(feature = "listenfd"), allow(dead_code))]
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// None if connection still hasn't been established.
    #[cfg(any(
        feature = "listenfd",
        feature = "sources-utils-tcp-keepalive",
        feature = "sources-utils-tcp-socket"
    ))]
    pub fn get_ref(&self) -> Option<&S> {
        use super::MaybeTls;

        match &self.state {
            StreamState::Accepted(stream) => Some(match stream {
                MaybeTls::Raw(s) => s,
                MaybeTls::Tls(s) => s.get_ref(),
            }),
            StreamState::Accepting(_) => None,
            StreamState::AcceptError(_) => None,
        }
    }
}

impl MaybeTlsIncomingStream<TcpStream> {
    pub(super) fn new(
        stream: TcpStream,
        peer_addr: SocketAddr,
        acceptor: Option<SslAcceptor>,
    ) -> Self {
        let state = match acceptor {
            Some(acceptor) => StreamState::Accepting(
                async move { tokio_openssl::accept(&acceptor, stream).await }.boxed(),
            ),
            None => StreamState::Accepted(MaybeTlsStream::Raw(stream)),
        };
        Self { peer_addr, state }
    }

    // Explicit handshake method
    #[cfg(feature = "listenfd")]
    pub(crate) async fn handshake(&mut self) -> crate::tls::Result<()> {
        if let StreamState::Accepting(fut) = &mut self.state {
            let stream = fut.await.context(Handshake)?;
            self.state = StreamState::Accepted(MaybeTlsStream::Tls(stream));
        }

        Ok(())
    }

    #[cfg(feature = "sources-utils-tcp-keepalive")]
    pub(crate) fn set_keepalive(&mut self, keepalive: TcpKeepaliveConfig) -> io::Result<()> {
        let stream = self.get_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotConnected,
                "Can't set keepalive on connection that has not been accepted yet.",
            )
        })?;

        stream.set_keepalive(keepalive.time_secs.map(std::time::Duration::from_secs))?;

        Ok(())
    }

    #[cfg(feature = "sources-utils-tcp-socket")]
    pub(crate) fn set_receive_buffer_bytes(&mut self, bytes: usize) -> std::io::Result<()> {
        let stream = self.get_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotConnected,
                "Can't set receive buffer size on connection that has not been accepted yet.",
            )
        })?;

        stream.set_recv_buffer_size(bytes)?;

        Ok(())
    }

    fn poll_io<T, F>(self: Pin<&mut Self>, cx: &mut Context, poll_fn: F) -> Poll<io::Result<T>>
    where
        F: FnOnce(Pin<&mut MaybeTlsStream<TcpStream>>, &mut Context) -> Poll<io::Result<T>>,
    {
        let mut this = self.get_mut();
        loop {
            return match &mut this.state {
                StreamState::Accepted(stream) => poll_fn(Pin::new(stream), cx),
                StreamState::Accepting(fut) => match futures::ready!(fut.as_mut().poll(cx)) {
                    Ok(stream) => {
                        this.state = StreamState::Accepted(MaybeTlsStream::Tls(stream));
                        continue;
                    }
                    Err(error) => {
                        let error = io::Error::new(io::ErrorKind::Other, error);
                        this.state = StreamState::AcceptError(error.to_string());
                        Poll::Ready(Err(error))
                    }
                },
                StreamState::AcceptError(error) => {
                    Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, error.to_owned())))
                }
            };
        }
    }
}

impl AsyncRead for MaybeTlsIncomingStream<TcpStream> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_io(cx, |s, cx| s.poll_read(cx, buf))
    }

    unsafe fn prepare_uninitialized_buffer(&self, _buf: &mut [MaybeUninit<u8>]) -> bool {
        // Both, TcpStream & SslStream return false
        // We can not use `poll_io` here, because need Context for polling handshake
        false
    }

    fn poll_read_buf<B: BufMut>(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        self.poll_io(cx, |s, cx| s.poll_read_buf(cx, buf))
    }
}

impl AsyncWrite for MaybeTlsIncomingStream<TcpStream> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.poll_io(cx, |s, cx| s.poll_write(cx, buf))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.poll_io(cx, |s, cx| s.poll_flush(cx))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.poll_io(cx, |s, cx| s.poll_shutdown(cx))
    }

    fn poll_write_buf<B: Buf>(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        self.poll_io(cx, |s, cx| s.poll_write_buf(cx, buf))
    }
}
