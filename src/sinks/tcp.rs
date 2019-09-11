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
use snafu::{ResultExt, Snafu};
use std::fmt::Debug;
use std::fs::File;
use std::io::Read;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::{
    codec::{BytesCodec, FramedWrite},
    net::tcp::{ConnectFuture, TcpStream},
    timer::Delay,
};
use tokio_retry::strategy::ExponentialBackoff;
use tokio_tls::{Connect as TlsConnect, TlsConnector, TlsStream};
use tracing::field;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Could not open {} file {:?}: {}", note, filename, source))]
    FileOpenFailed {
        note: &'static str,
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Could not read {} file {:?}: {}", note, filename, source))]
    FileReadFailed {
        note: &'static str,
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Must specify both TLS key_file and crt_file"))]
    MissingCrtKeyFile,
    #[snafu(display("Could not build TLS connector: {}", source))]
    TlsBuildError { source: native_tls::Error },
    #[snafu(display("Could not set TCP TLS identity: {}", source))]
    TlsIdentityError { source: native_tls::Error },
    #[snafu(display("Could not export identity to DER: {}", source))]
    DerExportError { source: openssl::error::ErrorStack },
    #[snafu(display("Could not parse certificate in {:?}: {}", filename, source))]
    CertificateParseError {
        filename: PathBuf,
        source: native_tls::Error,
    },
    #[snafu(display("Could not parse X509 certificate in {:?}: {}", filename, source))]
    X509ParseError {
        filename: PathBuf,
        source: openssl::error::ErrorStack,
    },
    #[snafu(display("Could not parse private key in {:?}: {}", filename, source))]
    PrivateKeyParseError {
        filename: PathBuf,
        source: openssl::error::ErrorStack,
    },
    #[snafu(display("Could not build PKCS#12 archive for identity: {}", source))]
    Pkcs12Error { source: openssl::error::ErrorStack },
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TcpSinkConfig {
    pub address: String,
    pub encoding: Option<Encoding>,
    pub tls: Option<TlsConfig>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

#[derive(Deserialize, Serialize, Debug, Default, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub struct TlsConfig {
    pub enabled: Option<bool>,
    pub verify: Option<bool>,
    pub crt_file: Option<String>,
    pub key_file: Option<String>,
    pub key_phrase: Option<String>,
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
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), crate::Error> {
        let addr = self
            .address
            .to_socket_addrs()
            .context(super::SocketAddressError)?
            .next()
            .ok_or(Box::new(super::BuildError::DNSFailure {
                address: self.address.clone(),
            }))?;

        let tls = match self.tls {
            Some(ref tls) => {
                if tls.enabled.unwrap_or(false) {
                    if tls.key_file.is_some() != tls.crt_file.is_some() {
                        return Err(Box::new(BuildError::MissingCrtKeyFile));
                    }
                    let add_ca = match &tls.ca_file {
                        None => None,
                        Some(filename) => Some(load_certificate(filename)?),
                    };
                    let identity = match &tls.crt_file {
                        None => None,
                        Some(filename) => {
                            // This unwrap is safe because of the crt/key check above
                            let key = load_key(tls.key_file.as_ref().unwrap(), &tls.key_phrase)?;
                            let crt = load_x509(filename)?;
                            Some(
                                Pkcs12::builder()
                                    .build("", filename, &key, &crt)
                                    .context(Pkcs12Error)?,
                            )
                        }
                    };
                    Some(TcpSinkTls {
                        verify: tls.verify.unwrap_or(true),
                        add_ca,
                        identity,
                    })
                } else {
                    None
                }
            }
            None => None,
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

fn load_certificate<T: AsRef<Path> + Debug>(filename: T) -> Result<Certificate, crate::Error> {
    let filename = filename.as_ref();
    let data = open_read(filename, "certificate authority")?;
    Ok(Certificate::from_pem(&data).with_context(|| CertificateParseError { filename })?)
}

fn load_key<T: AsRef<Path> + Debug>(
    filename: T,
    pass_phrase: &Option<String>,
) -> Result<PKey<Private>, crate::Error> {
    let filename = filename.as_ref();
    let data = open_read(filename, "key")?;
    match pass_phrase {
        None => {
            Ok(PKey::private_key_from_pem(&data)
                .with_context(|| PrivateKeyParseError { filename })?)
        }
        Some(phrase) => Ok(
            PKey::private_key_from_pem_passphrase(&data, phrase.as_bytes())
                .with_context(|| PrivateKeyParseError { filename })?,
        ),
    }
}

fn load_x509<T: AsRef<Path> + Debug>(filename: T) -> Result<X509, crate::Error> {
    let filename = filename.as_ref();
    let data = open_read(filename, "certificate")?;
    Ok(X509::from_pem(&data).with_context(|| X509ParseError { filename })?)
}

fn open_read<F: AsRef<Path> + Debug>(
    filename: F,
    note: &'static str,
) -> Result<Vec<u8>, crate::Error> {
    let mut text = Vec::<u8>::new();
    let filename = filename.as_ref();

    File::open(filename)
        .with_context(|| FileOpenFailed { note, filename })?
        .read_to_end(&mut text)
        .with_context(|| FileReadFailed { note, filename })?;

    Ok(text)
}

pub struct TcpSink {
    hostname: String,
    addr: SocketAddr,
    tls: Option<TcpSinkTls>,
    state: TcpSinkState,
    backoff: ExponentialBackoff,
}

enum TcpSinkState {
    Disconnected,
    Connecting(ConnectFuture),
    TlsConnecting(TlsConnect<TcpStream>),
    Connected(TcpOrTlsStream),
    Backoff(Delay),
}

type TcpOrTlsStream = MaybeTlsStream<
    FramedWrite<TcpStream, BytesCodec>,
    FramedWrite<TlsStream<TcpStream>, BytesCodec>,
>;

#[derive(Default)]
pub struct TcpSinkTls {
    verify: bool,
    add_ca: Option<Certificate>,
    identity: Option<Pkcs12>,
}

impl TcpSinkTls {
    fn make_connector(&self) -> Result<TlsConnector, crate::Error> {
        let mut connector = native_tls::TlsConnector::builder();
        connector.danger_accept_invalid_certs(!self.verify);
        if let Some(ref certificate) = self.add_ca {
            connector.add_root_certificate(certificate.clone());
        }
        if let Some(ref identity) = self.identity {
            let identity = Identity::from_pkcs12(&identity.to_der().context(DerExportError)?, "")
                .context(TlsIdentityError)?;
            connector.identity(identity);
        }
        Ok(TlsConnector::from(
            connector.build().context(TlsBuildError)?,
        ))
    }
}

impl TcpSink {
    pub fn new(hostname: String, addr: SocketAddr, tls: Option<TcpSinkTls>) -> Self {
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

    fn next_delay(&mut self) -> Delay {
        Delay::new(Instant::now() + self.backoff.next().unwrap())
    }

    fn poll_connection(&mut self) -> Poll<&mut TcpOrTlsStream, ()> {
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
                        match self.tls {
                            Some(ref tls) => match tls.make_connector() {
                                Ok(connector) => TcpSinkState::TlsConnecting(
                                    connector.connect(&self.hostname, socket),
                                ),
                                Err(err) => {
                                    error!(message = "unable to establish TLS connection.", error = %err);
                                    TcpSinkState::Backoff(self.next_delay())
                                }
                            },
                            None => TcpSinkState::Connected(MaybeTlsStream::Raw(FramedWrite::new(
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
                        TcpSinkState::Backoff(self.next_delay())
                    }
                },
                TcpSinkState::TlsConnecting(ref mut connect_future) => {
                    match connect_future.poll() {
                        Ok(Async::Ready(socket)) => {
                            debug!(message = "negotiated TLS.");
                            self.backoff = Self::fresh_backoff();
                            TcpSinkState::Connected(MaybeTlsStream::Tls(FramedWrite::new(
                                socket,
                                BytesCodec::new(),
                            )))
                        }
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(err) => {
                            error!(message = "unable to negotiate TLS.", addr = %self.addr, error = %err);
                            TcpSinkState::Backoff(self.next_delay())
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
                match connection.start_send(line) {
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

        match connection.poll_complete() {
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
    tls: Option<TcpSinkTls>,
) -> super::RouterSink {
    Box::new(
        TcpSink::new(hostname, addr, tls)
            .stream_ack(acker)
            .with(move |event| encode_event(event, &encoding)),
    )
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: std::io::Error },
}

pub fn tcp_healthcheck(addr: SocketAddr) -> super::Healthcheck {
    // Lazy to avoid immediately connecting
    let check = future::lazy(move || {
        TcpStream::connect(&addr)
            .map(|_| ())
            .map_err(|source| HealthcheckError::ConnectError { source }.into())
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

enum MaybeTlsStream<R, T> {
    Raw(R),
    Tls(T),
}

impl<R, T, I, E> Sink for MaybeTlsStream<R, T>
where
    R: Sink<SinkItem = I, SinkError = E>,
    T: Sink<SinkItem = I, SinkError = E>,
{
    type SinkItem = I;
    type SinkError = E;

    fn start_send(&mut self, item: I) -> futures::StartSend<I, E> {
        match self {
            MaybeTlsStream::Raw(r) => r.start_send(item),
            MaybeTlsStream::Tls(t) => t.start_send(item),
        }
    }

    fn poll_complete(&mut self) -> futures::Poll<(), E> {
        match self {
            MaybeTlsStream::Raw(r) => r.poll_complete(),
            MaybeTlsStream::Tls(t) => t.poll_complete(),
        }
    }
}
