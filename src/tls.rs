use futures01::Poll;
#[cfg(feature = "sources-tls")]
use futures01::{try_ready, Async, Future, Stream};
#[cfg(feature = "sources-tls")]
use openssl::ssl::{HandshakeError, SslAcceptor};
use openssl::{
    error::ErrorStack,
    pkcs12::{ParsedPkcs12, Pkcs12},
    pkey::{PKey, Private},
    ssl::{
        ConnectConfiguration, SslConnector, SslConnectorBuilder, SslContextBuilder, SslMethod,
        SslVerifyMode,
    },
    x509::{store::X509StoreBuilder, X509},
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::fmt::{self, Debug};
use std::fs::File;
use std::io::{self, Read, Write};
#[cfg(feature = "sources-tls")]
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncWrite};
#[cfg(feature = "sources-tls")]
use tokio::net::{tcp::Incoming, TcpListener, TcpStream};
use tokio_openssl::SslStream;
#[cfg(feature = "sources-tls")]
use tokio_openssl::{AcceptAsync, SslAcceptorExt};

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
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TlsConfig {
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub options: TlsOptions,
}

/// Standard TLS options
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TlsOptions {
    pub verify_certificate: Option<bool>,
    pub verify_hostname: Option<bool>,
    pub ca_path: Option<PathBuf>,
    pub crt_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub key_pass: Option<String>,
}

/// Directly usable settings for TLS connectors
#[derive(Clone, Default)]
pub struct TlsSettings {
    verify_certificate: bool,
    verify_hostname: bool,
    authority: Option<X509>,
    identity: Option<IdentityStore>, // openssl::pkcs12::ParsedPkcs12 doesn't impl Clone yet
}

#[derive(Clone)]
pub struct IdentityStore(Vec<u8>, String);

impl TlsSettings {
    /// Generate an optional settings struct from the given optional
    /// configuration reference. If `config` is `None`, TLS is
    /// disabled. The `for_server` parameter indicates the options
    /// should be interpreted as being for a TLS server, which requires
    /// an identity certificate and changes the certificate verification
    /// default to false.
    pub fn from_config(
        config: &Option<TlsConfig>,
        for_server: bool,
    ) -> crate::Result<Option<Self>> {
        match config {
            None => Ok(None), // No config, no TLS settings
            Some(config) => match config.enabled.unwrap_or(false) {
                false => Ok(None), // Explicitly disabled, still no TLS settings
                true => {
                    let tls = Self::from_options_base(&Some(config.options.clone()), for_server)?;
                    match (for_server, &tls.identity) {
                        // Servers require an identity certificate
                        (true, None) => Err(TlsError::MissingRequiredIdentity.into()),
                        _ => Ok(Some(tls)),
                    }
                }
            },
        }
    }

    /// Generate a filled out settings struct from the given optional
    /// option set, interpreted as client options. If `options` is
    /// `None`, the result is set to defaults (ie empty).
    pub fn from_options(options: &Option<TlsOptions>) -> crate::Result<Self> {
        Self::from_options_base(options, false)
    }

    fn from_options_base(options: &Option<TlsOptions>, for_server: bool) -> crate::Result<Self> {
        let default = TlsOptions::default();
        let options = options.as_ref().unwrap_or(&default);

        if !for_server {
            if options.verify_certificate == Some(false) {
                warn!(
                    "`verify_certificate` is DISABLED, this may lead to security vulnerabilities"
                );
            }
            if options.verify_hostname == Some(false) {
                warn!("`verify_hostname` is DISABLED, this may lead to security vulnerabilities");
            }
        }

        if options.key_path.is_some() && options.crt_path.is_none() {
            return Err(TlsError::MissingCrtKeyFile.into());
        }

        let authority = match options.ca_path {
            None => None,
            Some(ref path) => Some(load_x509(path)?),
        };

        let identity = match options.crt_path {
            None => None,
            Some(ref crt_path) => {
                let name = crt_path.to_string_lossy().to_string();
                let cert_data = open_read(crt_path, "certificate")?;
                let key_pass: &str = options.key_pass.as_ref().map(|s| s.as_str()).unwrap_or("");

                match Pkcs12::from_der(&cert_data) {
                    // Certificate file is DER encoded PKCS#12 archive
                    Ok(pkcs12) => {
                        // Verify password
                        pkcs12.parse(&key_pass)?;
                        Some(IdentityStore(cert_data, key_pass.to_string()))
                    }
                    Err(err) => {
                        if options.key_path.is_none() {
                            return Err(err.into());
                        }
                        // Identity is a PEM encoded certficate+key pair
                        let crt = load_x509(crt_path)?;
                        let key_path = options.key_path.as_ref().unwrap();
                        let key = load_key(&key_path, &options.key_pass)?;
                        let pkcs12 = Pkcs12::builder()
                            .build("", &name, &key, &crt)
                            .context(Pkcs12Error)?;
                        let identity = pkcs12.to_der().context(DerExportError)?;

                        // Build the resulting parsed PKCS#12 archive,
                        // but don't store it, as it cannot be cloned.
                        // This is just for error checking.
                        pkcs12.parse("").context(TlsIdentityError)?;

                        Some(IdentityStore(identity, "".into()))
                    }
                }
            }
        };

        Ok(Self {
            verify_certificate: options.verify_certificate.unwrap_or(!for_server),
            verify_hostname: options.verify_hostname.unwrap_or(!for_server),
            authority,
            identity,
        })
    }

    fn identity(&self) -> Option<ParsedPkcs12> {
        // This data was test-built previously, so we can just use it
        // here and expect the results will not fail. This can all be
        // reworked when `openssl::pkcs12::ParsedPkcs12` gains the Clone
        // impl.
        self.identity.as_ref().map(|identity| {
            Pkcs12::from_der(&identity.0)
                .expect("Could not build PKCS#12 archive from parsed data")
                .parse(&identity.1)
                .expect("Could not parse stored PKCS#12 archive")
        })
    }

    #[cfg(feature = "sources-tls")]
    pub(crate) fn acceptor(&self) -> crate::Result<SslAcceptor> {
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

    fn apply_context(&self, context: &mut SslContextBuilder) -> crate::Result<()> {
        context.set_verify(if self.verify_certificate {
            SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT
        } else {
            SslVerifyMode::NONE
        });
        if let Some(identity) = self.identity() {
            context
                .set_certificate(&identity.cert)
                .context(SetCertificate)?;
            context
                .set_private_key(&identity.pkey)
                .context(SetPrivateKey)?;
            if let Some(chain) = identity.chain {
                for cert in chain {
                    context
                        .add_extra_chain_cert(cert)
                        .context(AddExtraChainCert)?;
                }
            }
        }
        if let Some(certificate) = &self.authority {
            let mut store = X509StoreBuilder::new().context(NewStoreBuilder)?;
            store
                .add_cert(certificate.clone())
                .context(AddCertToStore)?;
            context
                .set_verify_cert_store(store.build())
                .context(SetVerifyCert)?;
        }
        Ok(())
    }
}

pub(crate) fn tls_connector_builder(
    settings: Option<TlsSettings>,
) -> crate::Result<SslConnectorBuilder> {
    let mut builder = SslConnector::builder(SslMethod::tls()).context(TlsBuildConnector)?;
    if let Some(settings) = settings {
        settings.apply_context(&mut builder)?;
    }
    Ok(builder)
}

pub(crate) fn tls_connector(settings: Option<TlsSettings>) -> crate::Result<ConnectConfiguration> {
    let verify_hostname = settings
        .as_ref()
        .map(|settings| settings.verify_hostname)
        .unwrap_or(true);
    let configure = tls_connector_builder(settings)?
        .build()
        .configure()
        .context(TlsBuildConnector)?
        .verify_hostname(verify_hostname);
    Ok(configure)
}

impl fmt::Debug for TlsSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsSettings")
            .field("verify_certificate", &self.verify_certificate)
            .field("verify_hostname", &self.verify_hostname)
            .finish()
    }
}

/// Load a private key from a named file
fn load_key(filename: &Path, pass_phrase: &Option<String>) -> crate::Result<PKey<Private>> {
    let data = open_read(filename, "key")?;
    match pass_phrase {
        None => Ok(PKey::private_key_from_der(&data)
            .or_else(|_| PKey::private_key_from_pem(&data))
            .with_context(|| PrivateKeyParseError { filename })?),
        Some(phrase) => Ok(
            PKey::private_key_from_pkcs8_passphrase(&data, phrase.as_bytes())
                .or_else(|_| PKey::private_key_from_pem_passphrase(&data, phrase.as_bytes()))
                .with_context(|| PrivateKeyParseError { filename })?,
        ),
    }
}

/// Load an X.509 certificate from a named file
fn load_x509(filename: &Path) -> crate::Result<X509> {
    let data = open_read(filename, "certificate")?;
    Ok(X509::from_der(&data)
        .or_else(|_| X509::from_pem(&data))
        .with_context(|| X509ParseError { filename })?)
}

fn open_read(filename: &Path, note: &'static str) -> crate::Result<Vec<u8>> {
    let mut text = Vec::<u8>::new();

    File::open(filename)
        .with_context(|| FileOpenFailed { note, filename })?
        .read_to_end(&mut text)
        .with_context(|| FileReadFailed { note, filename })?;

    Ok(text)
}

#[cfg(feature = "sources-tls")]
pub(crate) struct MaybeTlsIncoming<I: Stream> {
    incoming: I,
    acceptor: Option<SslAcceptor>,
    state: MaybeTlsIncomingState<I::Item>,
}

#[cfg(feature = "sources-tls")]
enum MaybeTlsIncomingState<S> {
    Inner,
    Accepting(AcceptAsync<S>),
}

#[cfg(feature = "sources-tls")]
impl<I: Stream> MaybeTlsIncoming<I> {
    #[cfg(feature = "sources-tls")]
    pub fn new(incoming: I, tls: Option<TlsSettings>) -> crate::Result<Self> {
        let acceptor = if let Some(tls) = tls {
            let acceptor = tls.acceptor()?;
            Some(acceptor.into())
        } else {
            None
        };

        let state = MaybeTlsIncomingState::Inner;

        Ok(Self {
            incoming,
            acceptor,
            state,
        })
    }
}

#[cfg(feature = "sources-tls")]
impl MaybeTlsIncoming<Incoming> {
    #[cfg(feature = "sources-tls")]
    pub fn bind(addr: &SocketAddr, tls: Option<TlsSettings>) -> crate::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let incoming = listener.incoming();

        MaybeTlsIncoming::new(incoming, tls)
    }
}

#[cfg(feature = "sources-tls")]
impl Stream for MaybeTlsIncoming<Incoming> {
    type Item = MaybeTlsStream<TcpStream>;
    type Error = TlsError;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            match &mut self.state {
                MaybeTlsIncomingState::Inner => {
                    let stream = if let Some(stream) = try_ready!(self
                        .incoming
                        .poll()
                        .map_err(Into::into)
                        .context(IncomingListener))
                    {
                        stream
                    } else {
                        return Ok(Async::Ready(None));
                    };

                    if let Some(acceptor) = &mut self.acceptor {
                        let fut = acceptor.accept_async(stream);

                        self.state = MaybeTlsIncomingState::Accepting(fut);
                        continue;
                    } else {
                        return Ok(Async::Ready(Some(MaybeTlsStream::Raw(stream))));
                    }
                }

                MaybeTlsIncomingState::Accepting(fut) => {
                    let stream = try_ready!(fut.poll().context(Handshake));
                    self.state = MaybeTlsIncomingState::Inner;

                    return Ok(Async::Ready(Some(MaybeTlsStream::Tls(stream))));
                }
            }
        }
    }
}

#[cfg(feature = "sources-tls")]
impl<I: Stream + fmt::Debug> fmt::Debug for MaybeTlsIncoming<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MaybeTlsIncoming")
            .field("incoming", &self.incoming)
            .finish()
    }
}

#[derive(Debug)]
pub enum MaybeTlsStream<S> {
    Tls(SslStream<S>),
    Raw(S),
}

impl<S: Read + Write> Read for MaybeTlsStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            MaybeTlsStream::Tls(s) => s.read(buf),
            MaybeTlsStream::Raw(s) => s.read(buf),
        }
    }
}

impl<S: AsyncRead + AsyncWrite> AsyncRead for MaybeTlsStream<S> {}

impl<S: Read + Write> Write for MaybeTlsStream<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            MaybeTlsStream::Tls(s) => s.write(buf),
            MaybeTlsStream::Raw(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            MaybeTlsStream::Tls(s) => s.flush(),
            MaybeTlsStream::Raw(s) => s.flush(),
        }
    }
}

impl<S: AsyncRead + AsyncWrite> AsyncWrite for MaybeTlsStream<S> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        match self {
            MaybeTlsStream::Tls(s) => s.shutdown(),
            MaybeTlsStream::Raw(s) => s.shutdown(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::assert_downcast_matches;

    const TEST_PKCS12: &str = "tests/data/localhost.p12";
    const TEST_PEM_CRT: &str = "tests/data/localhost.crt";
    const TEST_PEM_KEY: &str = "tests/data/localhost.key";

    #[test]
    fn from_options_pkcs12() {
        let options = TlsOptions {
            crt_path: Some(TEST_PKCS12.into()),
            key_pass: Some("NOPASS".into()),
            ..Default::default()
        };
        let settings =
            TlsSettings::from_options(&Some(options)).expect("Failed to load PKCS#12 certificate");
        assert!(settings.identity.is_some());
        assert!(settings.authority.is_none());
    }

    #[test]
    fn from_options_pem() {
        let options = TlsOptions {
            crt_path: Some(TEST_PEM_CRT.into()),
            key_path: Some(TEST_PEM_KEY.into()),
            ..Default::default()
        };
        let settings =
            TlsSettings::from_options(&Some(options)).expect("Failed to load PEM certificate");
        assert!(settings.identity.is_some());
        assert!(settings.authority.is_none());
    }

    #[test]
    fn from_options_ca() {
        let options = TlsOptions {
            ca_path: Some("tests/data/Vector_CA.crt".into()),
            ..Default::default()
        };
        let settings = TlsSettings::from_options(&Some(options))
            .expect("Failed to load authority certificate");
        assert!(settings.identity.is_none());
        assert!(settings.authority.is_some());
    }

    #[test]
    fn from_options_none() {
        let settings = TlsSettings::from_options(&None).expect("Failed to generate null settings");
        assert!(settings.identity.is_none());
        assert!(settings.authority.is_none());
    }

    #[test]
    fn from_options_bad_certificate() {
        let options = TlsOptions {
            key_path: Some(TEST_PEM_KEY.into()),
            ..Default::default()
        };
        let error = TlsSettings::from_options(&Some(options))
            .expect_err("from_options failed to check certificate");
        assert_downcast_matches!(error, TlsError, TlsError::MissingCrtKeyFile);

        let options = TlsOptions {
            crt_path: Some(TEST_PEM_CRT.into()),
            ..Default::default()
        };
        let _error = TlsSettings::from_options(&Some(options))
            .expect_err("from_options failed to check certificate");
        // Actual error is an ASN parse, doesn't really matter
    }

    #[test]
    fn from_config_none() {
        assert!(TlsSettings::from_config(&None, true).unwrap().is_none());
        assert!(TlsSettings::from_config(&None, false).unwrap().is_none());
    }

    #[test]
    fn from_config_not_enabled() {
        assert!(settings_from_config(None, false, false, true).is_none());
        assert!(settings_from_config(None, false, false, false).is_none());
        assert!(settings_from_config(Some(false), false, false, true).is_none());
        assert!(settings_from_config(Some(false), false, false, false).is_none());
    }

    #[test]
    fn from_config_fails_without_certificate() {
        let config = make_config(Some(true), false, false);
        let error = TlsSettings::from_config(&Some(config), true)
            .expect_err("from_config failed to check for a certificate");
        assert_downcast_matches!(error, TlsError, TlsError::MissingRequiredIdentity);
    }

    #[test]
    fn from_config_with_certificate() {
        let config = settings_from_config(Some(true), true, true, true);
        assert!(config.is_some());
    }

    fn settings_from_config(
        enabled: Option<bool>,
        set_crt: bool,
        set_key: bool,
        for_server: bool,
    ) -> Option<TlsSettings> {
        let config = make_config(enabled, set_crt, set_key);
        TlsSettings::from_config(&Some(config), for_server)
            .expect("Failed to generate settings from config")
    }

    fn make_config(enabled: Option<bool>, set_crt: bool, set_key: bool) -> TlsConfig {
        TlsConfig {
            enabled,
            options: TlsOptions {
                crt_path: and_some(set_crt, TEST_PEM_CRT.into()),
                key_path: and_some(set_key, TEST_PEM_KEY.into()),
                ..Default::default()
            },
        }
    }

    // This can be eliminated once the `bool_to_option` feature migrates
    // out of nightly.
    fn and_some<T>(src: bool, value: T) -> Option<T> {
        match src {
            true => Some(value),
            false => None,
        }
    }
}
