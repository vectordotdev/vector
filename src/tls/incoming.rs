use std::sync::Arc;
use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, stream, FutureExt, Stream};
use openssl::ssl::{Ssl, SslAcceptor, SslMethod};
use snafu::ResultExt;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::{
    io::{self, AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
};
use tokio_openssl::SslStream;

use super::{
    CreateAcceptor, Handshake, IncomingListener, MaybeTlsSettings, MaybeTlsStream, SslBuildError,
    TcpBind, TlsError, TlsSettings,
};
#[cfg(feature = "sources-utils-tcp-socket")]
use crate::tcp;
#[cfg(feature = "sources-utils-tcp-keepalive")]
use crate::tcp::TcpKeepaliveConfig;

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

pub struct MaybeTlsListener {
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

    #[allow(unused)]
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

    #[allow(unused)]
    pub(crate) fn accept_stream_limited(
        self,
        max_connections: Option<u32>,
    ) -> impl Stream<
        Item = (
            crate::tls::Result<MaybeTlsIncomingStream<TcpStream>>,
            Option<OwnedSemaphorePermit>,
        ),
    > {
        let connection_semaphore =
            max_connections.map(|max| Arc::new(Semaphore::new(max as usize)));

        let mut semaphore_future = connection_semaphore
            .clone()
            .map(|x| Box::pin(x.acquire_owned()));
        let mut accept = Box::pin(self.into_accept());
        stream::poll_fn(move |context| {
            let permit = match semaphore_future.as_mut() {
                Some(semaphore) => match semaphore.as_mut().poll(context) {
                    Poll::Ready(permit) => {
                        semaphore.set(connection_semaphore.clone().unwrap().acquire_owned());
                        permit.ok()
                    }
                    Poll::Pending => return Poll::Pending,
                },
                None => None,
            };
            match accept.as_mut().poll(context) {
                Poll::Ready((item, this)) => {
                    accept.set(this.into_accept());
                    Poll::Ready(Some((item, permit)))
                }
                Poll::Pending => Poll::Pending,
            }
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
    Accepting(BoxFuture<'static, Result<SslStream<S>, TlsError>>),
    AcceptError(String),
    Closed,
}

impl<S> MaybeTlsIncomingStream<S> {
    #[cfg_attr(not(feature = "listenfd"), allow(dead_code))]
    pub const fn peer_addr(&self) -> SocketAddr {
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
            StreamState::Closed => None,
        }
    }

    #[cfg(feature = "sources-vector")]
    pub(crate) const fn ssl_stream(&self) -> Option<&SslStream<S>> {
        use super::MaybeTls;

        match &self.state {
            StreamState::Accepted(stream) => match stream {
                MaybeTls::Raw(_) => None,
                MaybeTls::Tls(s) => Some(s),
            },
            StreamState::Accepting(_) | StreamState::AcceptError(_) | StreamState::Closed => None,
        }
    }

    #[cfg(all(
        test,
        feature = "sinks-socket",
        feature = "sources-utils-tls",
        feature = "listenfd"
    ))]
    pub fn get_mut(&mut self) -> Option<&mut S> {
        use super::MaybeTls;

        match &mut self.state {
            StreamState::Accepted(ref mut stream) => Some(match stream {
                MaybeTls::Raw(ref mut s) => s,
                MaybeTls::Tls(s) => s.get_mut(),
            }),
            StreamState::Accepting(_) => None,
            StreamState::AcceptError(_) => None,
            StreamState::Closed => None,
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
                async move {
                    let ssl = Ssl::new(acceptor.context()).context(SslBuildError)?;
                    let mut stream = SslStream::new(ssl, stream).context(SslBuildError)?;
                    Pin::new(&mut stream).accept().await.context(Handshake)?;
                    Ok(stream)
                }
                .boxed(),
            ),
            None => StreamState::Accepted(MaybeTlsStream::Raw(stream)),
        };
        Self { state, peer_addr }
    }

    // Explicit handshake method
    #[cfg(feature = "listenfd")]
    pub(crate) async fn handshake(&mut self) -> crate::tls::Result<()> {
        if let StreamState::Accepting(fut) = &mut self.state {
            let stream = fut.await?;
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

        if let Some(time_secs) = keepalive.time_secs {
            let config =
                socket2::TcpKeepalive::new().with_time(std::time::Duration::from_secs(time_secs));

            tcp::set_keepalive(stream, &config)?;
        }

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

        tcp::set_receive_buffer_size(stream, bytes)
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
                StreamState::Closed => Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
            };
        }
    }
}

impl AsyncRead for MaybeTlsIncomingStream<TcpStream> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll_io(cx, |s, cx| s.poll_read(cx, buf))
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
        let mut this = self.get_mut();
        match &mut this.state {
            StreamState::Accepted(stream) => match Pin::new(stream).poll_shutdown(cx) {
                Poll::Ready(Ok(())) => {
                    this.state = StreamState::Closed;
                    Poll::Ready(Ok(()))
                }
                poll_result => poll_result,
            },
            StreamState::Accepting(fut) => match futures::ready!(fut.as_mut().poll(cx)) {
                Ok(stream) => {
                    this.state = StreamState::Accepted(MaybeTlsStream::Tls(stream));
                    Poll::Pending
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
            StreamState::Closed => Poll::Ready(Ok(())),
        }
    }
}
