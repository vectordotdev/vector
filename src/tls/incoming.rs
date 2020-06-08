use super::{
    CreateAcceptor, IncomingListener, MaybeTlsSettings, MaybeTlsStream, PeerAddress, Result,
    TcpBind, TlsError, TlsSettings,
};
use futures01::{try_ready, Async, Future, Stream};
use openssl::ssl::{HandshakeError, SslAcceptor, SslMethod};
use snafu::ResultExt;
use std::{
    fmt::{self, Debug, Formatter},
    io::{self, ErrorKind, Read, Write},
    net::SocketAddr,
};
use tokio01::{
    io::{AsyncRead, AsyncWrite},
    net::{tcp::Incoming, TcpListener, TcpStream},
};
use tokio_openssl::{AcceptAsync, SslAcceptorExt};

pub(crate) struct MaybeTlsIncoming<I: Stream> {
    incoming: I,
    acceptor: Option<SslAcceptor>,
}

impl<I: Stream> MaybeTlsIncoming<I> {
    pub(crate) fn new(incoming: I, acceptor: Option<SslAcceptor>) -> Self {
        Self { incoming, acceptor }
    }
}

impl Stream for MaybeTlsIncoming<Incoming> {
    type Item = MaybeTlsIncomingStream<TcpStream>;
    type Error = TlsError;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>> {
        Ok(Async::Ready(
            match try_ready!(self
                .incoming
                .poll()
                .map_err(Into::into)
                .context(IncomingListener))
            {
                Some(stream) => Some(MaybeTlsIncomingStream::new(stream, &self.acceptor)?),
                None => None,
            },
        ))
    }
}

impl<I: Stream + Debug> Debug for MaybeTlsIncoming<I> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("MaybeTlsIncoming")
            .field("incoming", &self.incoming)
            .finish()
    }
}

impl TlsSettings {
    pub(crate) fn acceptor(&self) -> Result<SslAcceptor> {
        match self.identity {
            None => Err(TlsError::MissingRequiredIdentity.into()),
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
    pub(crate) fn bind(&self, addr: &SocketAddr) -> Result<MaybeTlsListener> {
        let listener = TcpListener::bind(addr).context(TcpBind)?;

        let acceptor = match self {
            Self::Tls(tls) => Some(tls.acceptor()?.into()),
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
    pub(crate) fn incoming(self) -> MaybeTlsIncoming<Incoming> {
        let incoming = self.listener.incoming();
        MaybeTlsIncoming::new(incoming, self.acceptor)
    }

    pub(crate) fn local_addr(&self) -> std::result::Result<SocketAddr, std::io::Error> {
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
    // AcceptAsync doesn't allow access to the inner stream, but users
    // of MaybeTlsIncomingStream want access to the peer address while
    // still handshaking, so we have to cache it here.
    peer_addr: SocketAddr,
}

enum StreamState<S> {
    Accepted(MaybeTlsStream<S>),
    Accepting(AcceptAsync<S>),
}

impl<S> MaybeTlsIncomingStream<S> {
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// None if connection still hasen't been established.
    pub fn get_ref(&self) -> Option<&S> {
        match &self.state {
            StreamState::Accepted(stream) => {
                if let Some(raw) = stream.raw() {
                    Some(raw)
                } else {
                    Some(
                        stream
                            .tls()
                            .expect("Stream not raw nor tls")
                            .get_ref()
                            .get_ref(),
                    )
                }
            }
            StreamState::Accepting(_) => None,
        }
    }
}

impl MaybeTlsIncomingStream<TcpStream> {
    pub(super) fn new(stream: TcpStream, acceptor: &Option<SslAcceptor>) -> Result<Self> {
        let peer_addr = stream.peer_addr().context(PeerAddress)?;
        let state = match acceptor {
            Some(acceptor) => StreamState::Accepting(acceptor.accept_async(stream)),
            None => StreamState::Accepted(MaybeTlsStream::Raw(stream)),
        };
        Ok(Self { peer_addr, state })
    }
}

fn poll_handshake<S: Read + Write>(acceptor: &mut AcceptAsync<S>) -> io::Result<StreamState<S>> {
    match acceptor.poll() {
        Err(error) => match error {
            HandshakeError::WouldBlock(_) => Err(io::Error::new(
                ErrorKind::WouldBlock,
                TlsError::HandshakeNotReady,
            )),
            HandshakeError::Failure(stream) => Err(io::Error::new(
                ErrorKind::Other,
                TlsError::Handshake {
                    source: stream.into_error(),
                },
            )),
            HandshakeError::SetupFailure(source) => Err(io::Error::new(
                ErrorKind::Other,
                TlsError::HandshakeSetup { source },
            )),
        },
        Ok(Async::Ready(stream)) => Ok(StreamState::Accepted(MaybeTlsStream::Tls(stream))),
        Ok(Async::NotReady) => Err(io::Error::new(
            ErrorKind::WouldBlock,
            TlsError::HandshakeNotReady,
        )),
    }
}

impl<S: Read + Write> Read for MaybeTlsIncomingStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.state {
            StreamState::Accepted(stream) => stream.read(buf),
            StreamState::Accepting(acceptor) => {
                self.state = poll_handshake(acceptor)?;
                self.read(buf)
            }
        }
    }
}

impl<S: Read + Write> Write for MaybeTlsIncomingStream<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.state {
            StreamState::Accepted(stream) => stream.write(buf),
            StreamState::Accepting(acceptor) => {
                self.state = poll_handshake(acceptor)?;
                self.write(buf)
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.state {
            StreamState::Accepted(stream) => stream.flush(),
            StreamState::Accepting(_) => Ok(()),
        }
    }
}

impl<S: Read + Write> AsyncRead for MaybeTlsIncomingStream<S> {}

impl<S: AsyncRead + AsyncWrite> AsyncWrite for MaybeTlsIncomingStream<S> {
    fn shutdown(&mut self) -> io::Result<Async<()>> {
        match &mut self.state {
            StreamState::Accepted(stream) => stream.shutdown(),
            StreamState::Accepting(_) => Ok(Async::Ready(())),
        }
    }
}
