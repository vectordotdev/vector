use super::{PeerAddress, TlsError};
use futures01::{Async, Future};
use openssl::ssl::SslAcceptor;
use snafu::ResultExt;
use std::{
    io::{self, ErrorKind, Read, Write},
    net::SocketAddr,
};
use tokio01::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_openssl::{AcceptAsync, SslAcceptorExt, SslStream};

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
    Accepting(AcceptAsync<S>),
}

impl<S> MaybeTlsStream<S> {
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.peer_addr)
    }
}

impl MaybeTlsStream<TcpStream> {
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

fn poll_handshake(acceptor: &mut AcceptAsync<TcpStream>) -> io::Result<State<TcpStream>> {
    match acceptor.poll() {
        Err(source) => Err(io::Error::new(
            ErrorKind::Other,
            TlsError::Handshake { source },
        )),
        Ok(Async::Ready(tls)) => Ok(State::Tls(tls)),
        Ok(Async::NotReady) => Err(io::Error::new(
            ErrorKind::WouldBlock,
            TlsError::HandshakeNotReady,
        )),
    }
}

impl Read for MaybeTlsStream<TcpStream> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.state {
            State::Raw(raw) => raw.read(buf),
            State::Tls(tls) => tls.read(buf),
            State::Accepting(acceptor) => {
                self.state = poll_handshake(acceptor)?;
                self.read(buf)
            }
        }
    }
}

impl Write for MaybeTlsStream<TcpStream> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.state {
            State::Raw(raw) => raw.write(buf),
            State::Tls(tls) => tls.write(buf),
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
            State::Accepting(_) => Ok(()),
        }
    }
}

impl AsyncRead for MaybeTlsStream<TcpStream> {}

impl AsyncWrite for MaybeTlsStream<TcpStream> {
    fn shutdown(&mut self) -> io::Result<Async<()>> {
        match &mut self.state {
            State::Raw(_) => Ok(Async::Ready(())),
            State::Tls(tls) => tls.shutdown(),
            State::Accepting(_) => Ok(Async::Ready(())),
        }
    }
}
