#[cfg(feature = "sources-tls")]
use openssl::ssl::HandshakeError;
use openssl::{
    error::ErrorStack,
    ssl::{ConnectConfiguration, SslConnector, SslConnectorBuilder, SslMethod},
};
use snafu::{ResultExt, Snafu};
use std::fmt::Debug;
use std::io::Error as IoError;
use std::path::PathBuf;
#[cfg(feature = "sources-tls")]
use tokio::net::TcpStream;

#[cfg(any(feature = "sources-tls", feature = "sources-http"))]
mod incoming;
mod maybe_tls;
mod settings;

#[cfg(any(feature = "sources-tls", feature = "sources-http"))]
pub(crate) use incoming::MaybeTlsStream;
pub(crate) use maybe_tls::MaybeTls;
pub use settings::{MaybeTlsSettings, TlsConfig, TlsOptions, TlsSettings};

pub type Result<T> = std::result::Result<T, TlsError>;

#[derive(Debug, Snafu)]
pub enum TlsError {
    #[snafu(display("Could not open {} file {:?}: {}", note, filename, source))]
    FileOpenFailed {
        note: &'static str,
        filename: PathBuf,
        source: IoError,
    },
    #[snafu(display("Could not read {} file {:?}: {}", note, filename, source))]
    FileReadFailed {
        note: &'static str,
        filename: PathBuf,
        source: IoError,
    },
    #[snafu(display("Could not build TLS connector: {}", source))]
    TlsBuildConnector { source: ErrorStack },
    #[snafu(display("Could not set TCP TLS identity: {}", source))]
    TlsIdentityError { source: ErrorStack },
    #[snafu(display("Could not export identity to DER: {}", source))]
    DerExportError { source: ErrorStack },
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
    #[cfg(feature = "sources-tls")]
    #[snafu(display("TLS handshake failed: {}", source))]
    Handshake { source: HandshakeError<TcpStream> },
    #[snafu(display("Incoming listener failed: {}", source))]
    IncomingListener { source: crate::Error },
    #[snafu(display("Creating the TLS acceptor failed: {}", source))]
    CreateAcceptor { source: ErrorStack },
    #[snafu(display("Error setting up the TLS certificate: {}", source))]
    SetCertificate { source: ErrorStack },
    #[snafu(display("Error setting up the TLS private key: {}", source))]
    SetPrivateKey { source: ErrorStack },
    #[snafu(display("Error setting up the TLS chain certificates: {}", source))]
    AddExtraChainCert { source: ErrorStack },
    #[snafu(display("Error creating a certificate store: {}", source))]
    NewStoreBuilder { source: ErrorStack },
    #[snafu(display("Error adding a certifcate to a store: {}", source))]
    AddCertToStore { source: ErrorStack },
    #[snafu(display("Error setting up the verification certificate: {}", source))]
    SetVerifyCert { source: ErrorStack },
    #[snafu(display("PKCS#12 parse failed: {}", source))]
    ParsePkcs12 { source: ErrorStack },
    #[snafu(display("TCP bind failed: {}", source))]
    TcpBind { source: IoError },
}

pub(crate) fn tls_connector_builder(settings: MaybeTlsSettings) -> Result<SslConnectorBuilder> {
    let mut builder = SslConnector::builder(SslMethod::tls()).context(TlsBuildConnector)?;
    if let Some(settings) = settings.tls() {
        settings.apply_context(&mut builder)?;
    }
    Ok(builder)
}

pub(crate) fn tls_connector(settings: MaybeTlsSettings) -> Result<ConnectConfiguration> {
    let verify_hostname = settings
        .tls()
        .map(|settings| settings.verify_hostname)
        .unwrap_or(true);
    let configure = tls_connector_builder(settings)?
        .build()
        .configure()
        .context(TlsBuildConnector)?
        .verify_hostname(verify_hostname);
    Ok(configure)
}
