#![allow(clippy::missing_errors_doc)]

use std::{fmt::Debug, net::SocketAddr, path::PathBuf, time::Duration};

use openssl::{
    error::ErrorStack,
    ssl::{ConnectConfiguration, SslConnector, SslConnectorBuilder, SslMethod},
};
use snafu::{ResultExt, Snafu};
use std::num::TryFromIntError;
use tokio::net::TcpStream;
use tokio_openssl::SslStream;

use crate::tcp::{self, TcpKeepaliveConfig};

mod incoming;
mod maybe_tls;
mod outgoing;
mod settings;

pub use incoming::{CertificateMetadata, MaybeTlsIncomingStream, MaybeTlsListener};
pub use maybe_tls::MaybeTls;
pub use settings::{
    MaybeTlsSettings, TlsConfig, TlsEnableableConfig, TlsSettings, TlsSourceConfig,
    PEM_START_MARKER, TEST_PEM_CA_PATH, TEST_PEM_CLIENT_CRT_PATH, TEST_PEM_CLIENT_KEY_PATH,
    TEST_PEM_CRT_PATH, TEST_PEM_INTERMEDIATE_CA_PATH, TEST_PEM_KEY_PATH,
};

pub type Result<T> = std::result::Result<T, TlsError>;

pub type MaybeTlsStream<S> = MaybeTls<S, SslStream<S>>;

#[derive(Debug, Snafu)]
pub enum TlsError {
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
    #[snafu(display("Could not build TLS connector: {}", source))]
    TlsBuildConnector { source: ErrorStack },
    #[snafu(display("Could not set TCP TLS identity: {}", source))]
    TlsIdentityError { source: ErrorStack },
    #[snafu(display("Could not export identity to DER: {}", source))]
    DerExportError { source: ErrorStack },
    #[snafu(display("Identity certificate is missing a key"))]
    MissingKey,
    #[snafu(display("Certificate file contains no certificates"))]
    MissingCertificate,
    #[snafu(display("Could not parse certificate in {:?}: {}", filename, source))]
    CertificateParseError {
        filename: PathBuf,
        source: ErrorStack,
    },
    #[snafu(display("Must specify both TLS key_file and crt_file"))]
    MissingCrtKeyFile,
    #[snafu(display("Could not parse X509 certificate in {:?}: {}", filename, source))]
    X509ParseError {
        filename: PathBuf,
        source: ErrorStack,
    },
    #[snafu(display("Could not parse private key in {:?}: {}", filename, source))]
    PrivateKeyParseError {
        filename: PathBuf,
        source: ErrorStack,
    },
    #[snafu(display("Could not build PKCS#12 archive for identity: {}", source))]
    Pkcs12Error { source: ErrorStack },
    #[snafu(display("Could not parse identity in {:?}: {}", filename, source))]
    IdentityParseError {
        filename: PathBuf,
        source: ErrorStack,
    },
    #[snafu(display("TLS configuration requires a certificate when enabled"))]
    MissingRequiredIdentity,
    #[snafu(display("TLS handshake failed: {}", source))]
    Handshake { source: openssl::ssl::Error },
    #[snafu(display("Incoming listener failed: {}", source))]
    IncomingListener { source: tokio::io::Error },
    #[snafu(display("Creating the TLS acceptor failed: {}", source))]
    CreateAcceptor { source: ErrorStack },
    #[snafu(display("Error building SSL context: {}", source))]
    SslBuildError { source: openssl::error::ErrorStack },
    #[snafu(display("Error setting up the TLS certificate: {}", source))]
    SetCertificate { source: ErrorStack },
    #[snafu(display("Error setting up the TLS private key: {}", source))]
    SetPrivateKey { source: ErrorStack },
    #[snafu(display("Error setting up the TLS chain certificates: {}", source))]
    AddExtraChainCert { source: ErrorStack },
    #[snafu(display("Error creating a certificate store: {}", source))]
    NewStoreBuilder { source: ErrorStack },
    #[snafu(display("Error adding a certificate to a store: {}", source))]
    AddCertToStore { source: ErrorStack },
    #[snafu(display("Error setting up the verification certificate: {}", source))]
    SetVerifyCert { source: ErrorStack },
    #[snafu(display("Error setting SNI: {}", source))]
    SetSni { source: ErrorStack },
    #[snafu(display("Error setting ALPN protocols: {}", source))]
    SetAlpnProtocols { source: ErrorStack },
    #[snafu(display(
        "Error encoding ALPN protocols, could not encode length as u8: {}",
        source
    ))]
    EncodeAlpnProtocols { source: TryFromIntError },
    #[snafu(display("PKCS#12 parse failed: {}", source))]
    ParsePkcs12 { source: ErrorStack },
    #[snafu(display("TCP bind failed: {}", source))]
    TcpBind { source: tokio::io::Error },
    #[snafu(display("{}", source))]
    Connect { source: tokio::io::Error },
    #[snafu(display("Could not get peer address: {}", source))]
    PeerAddress { source: std::io::Error },
    #[snafu(display("Security Framework Error: {}", source))]
    #[cfg(target_os = "macos")]
    SecurityFramework {
        source: security_framework::base::Error,
    },
    #[snafu(display("Schannel Error: {}", source))]
    #[cfg(windows)]
    Schannel { source: std::io::Error },
    #[cfg(any(windows, target_os = "macos"))]
    #[snafu(display("Unable to parse X509 from system cert: {}", source))]
    X509SystemParseError { source: ErrorStack },
    #[snafu(display("Creating an empty CA stack failed"))]
    NewCaStack { source: ErrorStack },
    #[snafu(display("Could not push intermediate certificate onto stack"))]
    CaStackPush { source: ErrorStack },
}

impl MaybeTlsStream<TcpStream> {
    pub fn peer_addr(&self) -> std::result::Result<SocketAddr, std::io::Error> {
        match self {
            Self::Raw(raw) => raw.peer_addr(),
            Self::Tls(tls) => tls.get_ref().peer_addr(),
        }
    }

    pub fn set_keepalive(&mut self, keepalive: TcpKeepaliveConfig) -> std::io::Result<()> {
        let stream = match self {
            Self::Raw(raw) => raw,
            Self::Tls(tls) => tls.get_ref(),
        };

        if let Some(time_secs) = keepalive.time_secs {
            let config = socket2::TcpKeepalive::new().with_time(Duration::from_secs(time_secs));

            tcp::set_keepalive(stream, &config)?;
        }

        Ok(())
    }

    pub fn set_send_buffer_bytes(&mut self, bytes: usize) -> std::io::Result<()> {
        let stream = match self {
            Self::Raw(raw) => raw,
            Self::Tls(tls) => tls.get_ref(),
        };

        tcp::set_send_buffer_size(stream, bytes)
    }

    pub fn set_receive_buffer_bytes(&mut self, bytes: usize) -> std::io::Result<()> {
        let stream = match self {
            Self::Raw(raw) => raw,
            Self::Tls(tls) => tls.get_ref(),
        };

        tcp::set_receive_buffer_size(stream, bytes)
    }
}

pub fn tls_connector_builder(settings: &MaybeTlsSettings) -> Result<SslConnectorBuilder> {
    let mut builder = SslConnector::builder(SslMethod::tls()).context(TlsBuildConnectorSnafu)?;
    if let Some(settings) = settings.tls() {
        settings.apply_context(&mut builder)?;
    }
    Ok(builder)
}

fn tls_connector(settings: &MaybeTlsSettings) -> Result<ConnectConfiguration> {
    let mut configure = tls_connector_builder(settings)?
        .build()
        .configure()
        .context(TlsBuildConnectorSnafu)?;
    let tls_setting = settings.tls().cloned();
    if let Some(tls_setting) = &tls_setting {
        tls_setting
            .apply_connect_configuration(&mut configure)
            .context(SetSniSnafu)?;
    }
    Ok(configure)
}
