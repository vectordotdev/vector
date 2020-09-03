use super::{CreateAcceptor, MaybeTlsSettings, MaybeTlsStream, TcpBind, TlsError, TlsSettings};
#[cfg(feature = "listenfd")]
use super::{Handshake, MaybeTls, PeerAddress};
use bytes::{Buf, BufMut};
use futures::{future::BoxFuture, FutureExt, Stream, StreamExt};
use openssl::ssl::{SslAcceptor, SslMethod};
use snafu::ResultExt;
use std::{
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
    pub(crate) fn incoming(
        &mut self,
    ) -> impl Stream<Item = crate::tls::Result<MaybeTlsIncomingStream<TcpStream>>> + '_ {
        let acceptor = self.acceptor.clone();
        self.listener
            .incoming()
            .map(move |connection| match connection {
                Ok(stream) => MaybeTlsIncomingStream::new(stream, acceptor.clone()),
                Err(source) => Err(TlsError::IncomingListener { source }),
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
    #[cfg(feature = "listenfd")]
    peer_addr: SocketAddr,
}

enum StreamState<S> {
    Accepted(MaybeTlsStream<S>),
    Accepting(BoxFuture<'static, Result<SslStream<S>, HandshakeError<S>>>),
    AcceptError(String),
}

#[cfg(feature = "listenfd")]
impl<S> MaybeTlsIncomingStream<S> {
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// None if connection still hasn't been established.
    pub fn get_ref(&self) -> Option<&S> {
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
    #[cfg(feature = "listenfd")]
    pub(super) fn new(
        stream: TcpStream,
        acceptor: Option<SslAcceptor>,
    ) -> crate::tls::Result<Self> {
        let peer_addr = stream.peer_addr().context(PeerAddress)?;
        let state = match acceptor {
            Some(acceptor) => StreamState::Accepting(
                async move { tokio_openssl::accept(&acceptor, stream).await }.boxed(),
            ),
            None => StreamState::Accepted(MaybeTlsStream::Raw(stream)),
        };
        Ok(Self { peer_addr, state })
    }

    #[cfg(not(feature = "listenfd"))]
    pub(super) fn new(
        stream: TcpStream,
        acceptor: Option<SslAcceptor>,
    ) -> crate::tls::Result<Self> {
        let state = match acceptor {
            Some(acceptor) => StreamState::Accepting(
                async move { tokio_openssl::accept(&acceptor, stream).await }.boxed(),
            ),
            None => StreamState::Accepted(MaybeTlsStream::Raw(stream)),
        };
        Ok(Self { state })
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
