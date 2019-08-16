use crate::{
    buffers::Acker,
    event::{self, Event},
    sinks::util::SinkExt,
    topology::config::{DataType, SinkConfig},
};
use bytes::Bytes;
use futures::{future, try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend};
use native_tls::{Certificate, Identity};
use openssl::{
    pkcs12::Pkcs12,
    pkey::{PKey, Private},
    x509::X509,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::Debug;
use std::fs::File;
use std::io::Read;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::{
    codec::{BytesCodec, FramedWrite},
    net::tcp::{ConnectFuture, TcpStream},
    prelude::AsyncWrite,
    timer::Delay,
};
use tokio_retry::strategy::ExponentialBackoff;
use tokio_tls::{Connect as TlsConnect, TlsConnector};
use tracing::field;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TcpSinkConfig {
    pub address: String,
    pub encoding: Option<Encoding>,
    pub tls: Option<TcpSinkTlsConfig>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

#[derive(Deserialize, Serialize, Debug, Default, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub struct TcpSinkTlsConfig {
    pub enabled: Option<bool>,
    pub verify: Option<bool>,
    pub crt_file: Option<String>,
    pub key_file: Option<String>,
    pub ca_file: Option<String>,
}

impl TcpSinkConfig {
    pub fn new(address: String) -> Self {
        Self {
            address,
            encoding: None,
            tls: None,
        }
    }
}

#[typetag::serde(name = "tcp")]
impl SinkConfig for TcpSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let addr = self
            .address
            .to_socket_addrs()
            .map_err(|e| format!("IO Error: {}", e))?
            .next()
            .ok_or_else(|| "Unable to resolve DNS for provided address".to_string())?;

        let tls = match self.tls {
            Some(ref tls) => {
                if tls.key_file.is_some() != tls.crt_file.is_some() {
                    return Err("Must specify both tls key_file and crt_file".into());
                }
                let add_ca = match tls.ca_file {
                    None => None,
                    Some(ref filename) => Some(load_certificate(filename)?),
                };
                let identity =
                    match tls.crt_file {
                        None => None,
                        Some(ref filename) => {
                            // This unwrap is safe because of the crt/key check above
                            let key = load_key(tls.key_file.as_ref().unwrap())?;
                            let crt = load_x509(filename)?;
                            Some(Pkcs12::builder().build("", "FIXME", &key, &crt).map_err(
                                |err| {
                                    format!("Could not build PKCS#12 archive for identity: {}", err)
                                },
                            )?)
                        }
                    };
                TcpSinkTls {
                    enabled: tls.enabled.unwrap_or(false),
                    verify: tls.verify.unwrap_or(true),
                    add_ca,
                    identity,
                }
            }
            None => TcpSinkTls::default(),
        };

        let sink = raw_tcp(
            self.address.clone(),
            addr,
            acker,
            self.encoding.clone(),
            tls,
        );
        let healthcheck = tcp_healthcheck(addr);

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }
}

fn load_certificate<T: AsRef<Path> + Debug>(filename: T) -> Result<Certificate, String> {
    open_read_parse(filename, "certificate authority", Certificate::from_pem)
}

fn load_key<T: AsRef<Path> + Debug>(filename: T) -> Result<PKey<Private>, String> {
    open_read_parse(filename, "key", PKey::private_key_from_pem)
}

fn load_x509<T: AsRef<Path> + Debug>(filename: T) -> Result<X509, String> {
    open_read_parse(filename, "certificate", X509::from_pem)
}

fn open_read_parse<F: AsRef<Path> + Debug, O, E: Error>(
    filename: F,
    note: &str,
    parser: fn(&[u8]) -> Result<O, E>,
) -> Result<O, String> {
    let mut text = Vec::<u8>::new();

    File::open(filename.as_ref())
        .map_err(|err| format!("Could not open {} file {:?}: {}", note, filename, err))?
        .read_to_end(&mut text)
        .map_err(|err| format!("Could not read {} file {:?}: {}", note, filename, err))?;

    parser(&text).map_err(|err| format!("Could not parse {} file {:?}: {}", note, filename, err))
}

pub struct TcpSink {
    hostname: String,
    addr: SocketAddr,
    tls: TcpSinkTls,
    state: TcpSinkState,
    backoff: ExponentialBackoff,
}

enum TcpSinkState {
    Disconnected,
    Connecting(ConnectFuture),
    TlsConnecting(TlsConnect<TcpStream>),
    Connected(Box<dyn FramedConnection + Send>),
    Backoff(Delay),
}

#[derive(Default)]
pub struct TcpSinkTls {
    enabled: bool,
    verify: bool,
    add_ca: Option<Certificate>,
    identity: Option<Pkcs12>,
}

impl TcpSink {
    pub fn new(hostname: String, addr: SocketAddr, tls: TcpSinkTls) -> Self {
        Self {
            hostname,
            addr,
            tls,
            state: TcpSinkState::Disconnected,
            backoff: Self::fresh_backoff(),
        }
    }

    fn fresh_backoff() -> ExponentialBackoff {
        // TODO: make configurable
        ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
    }

    fn poll_connection(&mut self) -> Poll<&mut Box<dyn FramedConnection + Send>, ()> {
        loop {
            self.state = match self.state {
                TcpSinkState::Disconnected => {
                    debug!(message = "connecting", addr = &field::display(&self.addr));
                    TcpSinkState::Connecting(TcpStream::connect(&self.addr))
                }
                TcpSinkState::Backoff(ref mut delay) => match delay.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    // Err can only occur if the tokio runtime has been shutdown or if more than 2^63 timers have been created
                    Err(err) => unreachable!(err),
                    Ok(Async::Ready(())) => {
                        debug!(
                            message = "disconnected.",
                            addr = &field::display(&self.addr)
                        );
                        TcpSinkState::Disconnected
                    }
                },
                TcpSinkState::Connecting(ref mut connect_future) => match connect_future.poll() {
                    Ok(Async::Ready(socket)) => {
                        let addr = socket.peer_addr().unwrap_or(self.addr);
                        debug!(message = "connected", addr = &field::display(&addr));
                        self.backoff = Self::fresh_backoff();
                        match self.tls.enabled {
                            true => {
                                let mut connector = native_tls::TlsConnector::builder();
                                connector.danger_accept_invalid_certs(!self.tls.verify);
                                if let Some(ref certificate) = self.tls.add_ca {
                                    connector.add_root_certificate(certificate.clone());
                                }
                                if let Some(ref identity) = self.tls.identity {
                                    connector.identity(
                                        Identity::from_pkcs12(
                                            &identity.to_der().unwrap_or_else(|err| {
                                                panic!("Could not export identity to DER: {}", err)
                                            }),
                                            "",
                                        )
                                        .unwrap_or_else(
                                            |err| panic!("Could not set TCP TLS identity: {}", err),
                                        ),
                                    );
                                }
                                let connector =
                                    connector.build().expect("Could not build TLS connector?!?");
                                TcpSinkState::TlsConnecting(
                                    TlsConnector::from(connector).connect(&self.hostname, socket),
                                )
                            }
                            false => TcpSinkState::Connected(Box::new(FramedWrite::new(
                                socket,
                                BytesCodec::new(),
                            ))),
                        }
                    }
                    Ok(Async::NotReady) => {
                        return Ok(Async::NotReady);
                    }
                    Err(err) => {
                        error!("Error connecting to {}: {}", self.addr, err);
                        let delay = Delay::new(Instant::now() + self.backoff.next().unwrap());
                        TcpSinkState::Backoff(delay)
                    }
                },
                TcpSinkState::TlsConnecting(ref mut connect_future) => {
                    match connect_future.poll() {
                        Ok(Async::Ready(socket)) => {
                            debug!(message = "negotiated TLS");
                            self.backoff = Self::fresh_backoff();
                            TcpSinkState::Connected(Box::new(FramedWrite::new(
                                socket,
                                BytesCodec::new(),
                            )))
                        }
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(err) => {
                            error!("Error negotiating TLS with {}: {}", self.addr, err);
                            let delay = Delay::new(Instant::now() + self.backoff.next().unwrap());
                            TcpSinkState::Backoff(delay)
                        }
                    }
                }
                TcpSinkState::Connected(ref mut connection) => {
                    return Ok(Async::Ready(connection));
                }
            };
        }
    }
}

impl Sink for TcpSink {
    type SinkItem = Bytes;
    type SinkError = ();

    fn start_send(&mut self, line: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match self.poll_connection() {
            Ok(Async::Ready(connection)) => {
                debug!(
                    message = "sending event.",
                    bytes = &field::display(line.len())
                );
                match connection.start(line) {
                    Err(err) => {
                        debug!(
                            message = "disconnected.",
                            addr = &field::display(&self.addr)
                        );
                        error!("Error in connection {}: {}", self.addr, err);
                        self.state = TcpSinkState::Disconnected;
                        Ok(AsyncSink::Ready)
                    }
                    Ok(ok) => Ok(ok),
                }
            }
            Ok(Async::NotReady) => Ok(AsyncSink::NotReady(line)),
            Err(_) => unreachable!(),
        }
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        // Stream::forward will immediately poll_complete the sink it's forwarding to,
        // but we don't want to connect before the first event actually comes through.
        if let TcpSinkState::Disconnected = self.state {
            return Ok(Async::Ready(()));
        }

        let connection = try_ready!(self.poll_connection());

        match connection.poll() {
            Err(err) => {
                debug!(
                    message = "disconnected.",
                    addr = &field::display(&self.addr)
                );
                error!("Error in connection {}: {}", self.addr, err);
                self.state = TcpSinkState::Disconnected;
                Ok(Async::Ready(()))
            }
            Ok(ok) => Ok(ok),
        }
    }
}

pub fn raw_tcp(
    hostname: String,
    addr: SocketAddr,
    acker: Acker,
    encoding: Option<Encoding>,
    tls: TcpSinkTls,
) -> super::RouterSink {
    Box::new(
        TcpSink::new(hostname, addr, tls)
            .stream_ack(acker)
            .with(move |event| encode_event(event, &encoding)),
    )
}

pub fn tcp_healthcheck(addr: SocketAddr) -> super::Healthcheck {
    // Lazy to avoid immediately connecting
    let check = future::lazy(move || {
        TcpStream::connect(&addr)
            .map(|_| ())
            .map_err(|err| err.to_string())
    });

    Box::new(check)
}

fn encode_event(event: Event, encoding: &Option<Encoding>) -> Result<Bytes, ()> {
    let log = event.into_log();

    let b = match (encoding, log.is_structured()) {
        (&Some(Encoding::Json), _) | (_, true) => {
            serde_json::to_vec(&log.unflatten()).map_err(|e| panic!("Error encoding: {}", e))
        }
        (&Some(Encoding::Text), _) | (_, false) => {
            let bytes = log
                .get(&event::MESSAGE)
                .map(|v| v.as_bytes().to_vec())
                .unwrap_or(Vec::new());
            Ok(bytes)
        }
    };

    b.map(|mut b| {
        b.push(b'\n');
        Bytes::from(b)
    })
}

trait FramedConnection {
    fn start(&mut self, line: Bytes) -> std::io::Result<AsyncSink<Bytes>>;
    fn poll(&mut self) -> std::io::Result<Async<()>>;
}

impl<T: AsyncWrite> FramedConnection for FramedWrite<T, BytesCodec> {
    fn start(&mut self, line: Bytes) -> std::io::Result<AsyncSink<Bytes>> {
        FramedWrite::<T, BytesCodec>::start_send(self, line)
    }
    fn poll(&mut self) -> std::io::Result<Async<()>> {
        FramedWrite::<T, BytesCodec>::poll_complete(self)
    }
}
