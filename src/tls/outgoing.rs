use super::{tls_connector, MaybeTlsSettings, MaybeTlsStream, Result, TlsError};
use futures01::{Async, Future};
use openssl::ssl::{ConnectConfiguration, HandshakeError};
use std::net::SocketAddr;
use tokio01::net::tcp::{ConnectFuture, TcpStream};
use tokio_openssl::{ConnectAsync, ConnectConfigurationExt};

enum State {
    Connecting(ConnectFuture, Option<ConnectConfiguration>),
    Negotiating(ConnectAsync<TcpStream>),
}

/// This is an asynchronous connection future that will optionally
/// negotiate a TLS session before returning a ready connection.
pub(crate) struct MaybeTlsConnector {
    host: String,
    state: State,
}

impl MaybeTlsConnector {
    fn new(host: String, addr: SocketAddr, tls: &MaybeTlsSettings) -> Result<Self> {
        let connector = TcpStream::connect(&addr);
        let tls_connector = match tls {
            MaybeTlsSettings::Raw(()) => None,
            MaybeTlsSettings::Tls(_) => Some(tls_connector(tls)?),
        };
        let state = State::Connecting(connector, tls_connector);
        Ok(Self { host, state })
    }
}

impl Future for MaybeTlsConnector {
    type Item = MaybeTlsStream<TcpStream>;
    type Error = TlsError;

    fn poll(&mut self) -> std::result::Result<Async<Self::Item>, Self::Error> {
        loop {
            match &mut self.state {
                State::Connecting(connector, tls) => match connector.poll() {
                    Err(source) => return Err(TlsError::Connect { source }),
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Ok(Async::Ready(stream)) => match tls.take() {
                        // Here's where the magic happens. If there is
                        // no TLS connector, just return ready with the
                        // raw stream. Otherwise, start the TLS
                        // negotiation and switch to that state.
                        None => return Ok(Async::Ready(MaybeTlsStream::Raw(stream))),
                        Some(tls_config) => {
                            let connector = tls_config.connect_async(&self.host, stream);
                            self.state = State::Negotiating(connector)
                        }
                    },
                },
                State::Negotiating(connector) => match connector.poll() {
                    Err(error) => {
                        return Err(match error {
                            HandshakeError::WouldBlock(_) => TlsError::HandshakeNotReady,
                            HandshakeError::Failure(stream) => TlsError::Handshake {
                                source: stream.into_error(),
                            },
                            HandshakeError::SetupFailure(source) => {
                                TlsError::HandshakeSetup { source }
                            }
                        })
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Ok(Async::Ready(stream)) => {
                        debug!(message = "negotiated TLS");
                        return Ok(Async::Ready(MaybeTlsStream::Tls(stream)));
                    }
                },
            }
        }
    }
}

impl MaybeTlsSettings {
    pub(crate) fn connect(&self, host: String, addr: SocketAddr) -> Result<MaybeTlsConnector> {
        MaybeTlsConnector::new(host, addr, self)
    }
}
