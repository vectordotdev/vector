use super::{PeerAddress, TlsError};
use futures01::Async;
#[cfg(feature = "sources-tls")]
use futures01::Future;
#[cfg(feature = "sources-tls")]
use openssl::ssl::{HandshakeError, SslAcceptor};
use snafu::ResultExt;
#[cfg(feature = "sources-tls")]
use std::io::ErrorKind;
use std::{
    io::{self, Read, Write},
    net::SocketAddr,
};
use tokio01::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_openssl::SslStream;
#[cfg(feature = "sources-tls")]
use tokio_openssl::{AcceptAsync, SslAcceptorExt};

pub struct MaybeTlsStream<S> {
    state: State<S>,
    // AcceptAsync doesn't allow access to the inner stream, but users
    // of MaybeTlsStream want access to the peer address while still
    // handshaking, so we have to cache it here.
    peer_addr: SocketAddr,
}

enum State<S> {
    Raw(S),
    Tls(SslStream<S>),
    #[cfg(feature = "sources-tls")]
    Accepting(AcceptAsync<S>),
}

impl<S> MaybeTlsStream<S> {
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }
}

impl MaybeTlsStream<TcpStream> {
    #[cfg(feature = "sources-tls")]
    pub(super) fn new_accepting(
        stream: TcpStream,
        acceptor: &Option<SslAcceptor>,
    ) -> Result<Self, TlsError> {
        let peer_addr = stream.peer_addr().context(PeerAddress)?;
        let state = match acceptor {
            Some(acceptor) => State::Accepting(acceptor.accept_async(stream)),
            None => State::Raw(stream),
        };
        Ok(Self { peer_addr, state })
    }

    pub(super) fn new_raw(stream: TcpStream) -> Result<Self, TlsError> {
        let peer_addr = stream.peer_addr().context(PeerAddress)?;
        let state = State::Raw(stream);
        Ok(Self { peer_addr, state })
    }

    pub(super) fn new_tls(stream: SslStream<TcpStream>) -> Result<Self, TlsError> {
        let peer_addr = stream
            .get_ref()
            .get_ref()
            .peer_addr()
            .context(PeerAddress)?;
        let state = State::Tls(stream);
        Ok(Self { peer_addr, state })
    }
}

#[cfg(feature = "sources-tls")]
fn poll_handshake<S: Read + Write>(acceptor: &mut AcceptAsync<S>) -> io::Result<State<S>> {
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
        Ok(Async::Ready(tls)) => Ok(State::Tls(tls)),
        Ok(Async::NotReady) => Err(io::Error::new(
            ErrorKind::WouldBlock,
            TlsError::HandshakeNotReady,
        )),
    }
}

impl<S: Read + Write> Read for MaybeTlsStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.state {
            State::Raw(raw) => raw.read(buf),
            State::Tls(tls) => tls.read(buf),
            #[cfg(feature = "sources-tls")]
            State::Accepting(acceptor) => {
                self.state = poll_handshake(acceptor)?;
                self.read(buf)
            }
        }
    }
}

impl<S: Read + Write> Write for MaybeTlsStream<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.state {
            State::Raw(raw) => raw.write(buf),
            State::Tls(tls) => tls.write(buf),
            #[cfg(feature = "sources-tls")]
            State::Accepting(acceptor) => {
                self.state = poll_handshake(acceptor)?;
                self.write(buf)
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.state {
            State::Raw(raw) => raw.flush(),
            State::Tls(tls) => tls.flush(),
            #[cfg(feature = "sources-tls")]
            State::Accepting(_) => Ok(()),
        }
    }
}

impl<S: Read + Write> AsyncRead for MaybeTlsStream<S> {}

impl<S: AsyncRead + AsyncWrite> AsyncWrite for MaybeTlsStream<S> {
    fn shutdown(&mut self) -> io::Result<Async<()>> {
        match &mut self.state {
            State::Raw(_) => Ok(Async::Ready(())),
            State::Tls(tls) => tls.shutdown(),
            #[cfg(feature = "sources-tls")]
            State::Accepting(_) => Ok(Async::Ready(())),
        }
    }
}
