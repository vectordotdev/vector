#![allow(clippy::missing_errors_doc)]

use std::{fmt::Debug, net::SocketAddr, num::TryFromIntError, path::PathBuf, time::Duration};

use openssl::{
    error::ErrorStack,
    ssl::{ConnectConfiguration, SslConnector, SslConnectorBuilder, SslMethod},
};
use snafu::{ResultExt, Snafu};
use tokio::net::TcpStream;
use tokio_openssl::SslStream;

use crate::tcp::{self, TcpKeepaliveConfig};

mod incoming;
mod maybe_tls;
mod outgoing;
mod reload;
mod settings;

pub use incoming::{CertificateMetadata, MaybeTlsIncomingStream, MaybeTlsListener};
pub use maybe_tls::MaybeTls;
pub use reload::{TlsAcceptorReloader, WeakTlsAcceptorReloader};
pub use settings::{
    MaybeTlsSettings, PEM_START_MARKER, TEST_PEM_CA_PATH, TEST_PEM_CLIENT_CRT_PATH,
    TEST_PEM_CLIENT_KEY_PATH, TEST_PEM_CRT_PATH, TEST_PEM_INTERMEDIATE_CA_PATH, TEST_PEM_KEY_PATH,
    TlsConfig, TlsEnableableConfig, TlsSettings, TlsSourceConfig, TlsVersion,
    host_protocol_version_range,
};

/// Install Ring as the process-wide Rustls provider if none is already installed.
///
/// Call before constructing TLS clients when multiple providers are enabled.
/// Repeated or concurrent calls preserve the first installed provider.
pub fn install_default_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        _ = rustls::crypto::ring::default_provider().install_default();
    }
}

pub type Result<T> = std::result::Result<T, TlsError>;

/// The protocol version range every rustls-based stack this crate talks to negotiates, and the
/// range `native-tls` pins itself to regardless of host policy (see `amqp`'s caller).
pub const TLS12_TO_TLS13: (TlsVersion, TlsVersion) = (TlsVersion::Tls12, TlsVersion::Tls13);

/// Fails unless `tls_stack`'s negotiable range guarantees the configured `min`/`max` bounds.
///
/// `min_version`/`max_version` are normally applied to the OpenSSL context built by
/// [`TlsSettings::apply_context_base`]. Components that instead hand the configured certificates
/// to a third-party TLS implementation never reach that code, so a bound only holds if
/// `tls_stack` can never negotiate outside it on its own; `negotiable_range` is the
/// `(lowest, highest)` version the stack may pick regardless of what Vector asks for.
pub fn enforce_protocol_version_bounds(
    min: Option<TlsVersion>,
    max: Option<TlsVersion>,
    tls_stack: &str,
    negotiable_range: (TlsVersion, TlsVersion),
) -> Result<()> {
    let (lowest, highest) = negotiable_range;
    let min = min.filter(|&min| min > lowest);
    let max = max.filter(|&max| max < highest);
    if min.is_none() && max.is_none() {
        return Ok(());
    }

    let bound = match (min, max) {
        (Some(min), Some(max)) => format!("`tls.min_version` {min} and `tls.max_version` {max}"),
        (Some(min), None) => format!("`tls.min_version` {min}"),
        (None, Some(max)) => format!("`tls.max_version` {max}"),
        (None, None) => unreachable!("checked above"),
    };
    let plural = if min.is_some() && max.is_some() {
        "s"
    } else {
        ""
    };
    Err(TlsError::ProtocolVersionUnenforceable {
        message: format!(
            "{bound} cannot be enforced by {tls_stack}, which may negotiate {lowest} to {highest}. Widen or remove the bound{plural}."
        ),
    })
}

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
    #[snafu(display("Error setting the TLS protocol version: {}", source))]
    SetTlsVersion { source: ErrorStack },
    #[snafu(display(
        "`min_version` ({}) must not be greater than `max_version` ({})",
        min,
        max
    ))]
    InvalidTlsVersionRange { min: TlsVersion, max: TlsVersion },
    #[snafu(display(
        "No TLS protocol version is usable: the host's OpenSSL configuration allows none within \
         `min_version`/`max_version`. OpenSSL security levels above 0 also disable TLS v1.0 and \
         v1.1."
    ))]
    EmptyTlsVersionWindow,
    #[snafu(display("{}", message))]
    ProtocolVersionUnenforceable { message: String },
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
            .apply_connect_configuration(&mut configure, false)
            .context(SetSniSnafu)?;
    }
    Ok(configure)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn unset_bounds_are_always_enforceable() {
        enforce_protocol_version_bounds(None, None, "rumqttc", TLS12_TO_TLS13).unwrap();
    }

    #[test]
    fn a_minimum_within_the_negotiable_floor_is_enforceable() {
        for min in [TlsVersion::Tls10, TlsVersion::Tls11, TlsVersion::Tls12] {
            enforce_protocol_version_bounds(Some(min), None, "async-nats", TLS12_TO_TLS13)
                .unwrap_or_else(|error| panic!("{min} should be enforceable: {error}"));
        }
    }

    #[test]
    fn a_minimum_above_the_negotiable_floor_is_unenforceable() {
        let error = enforce_protocol_version_bounds(
            Some(TlsVersion::Tls13),
            None,
            "async-nats",
            TLS12_TO_TLS13,
        )
        .expect_err("a minimum the stack might not reach should be rejected");
        assert!(
            matches!(error, TlsError::ProtocolVersionUnenforceable { .. }),
            "unexpected error: {error}"
        );
        assert!(error.to_string().contains("`tls.min_version` TLSv1.3"));
    }

    #[test]
    fn a_maximum_at_the_negotiable_ceiling_is_enforceable() {
        enforce_protocol_version_bounds(
            None,
            Some(TlsVersion::Tls13),
            "async-nats",
            TLS12_TO_TLS13,
        )
        .unwrap();
    }

    #[test]
    fn a_maximum_below_the_negotiable_ceiling_is_unenforceable() {
        let error = enforce_protocol_version_bounds(
            None,
            Some(TlsVersion::Tls12),
            "async-nats",
            TLS12_TO_TLS13,
        )
        .expect_err("a maximum the stack might exceed should be rejected");
        assert!(
            matches!(error, TlsError::ProtocolVersionUnenforceable { .. }),
            "unexpected error: {error}"
        );
        assert!(error.to_string().contains("`tls.max_version` TLSv1.2"));
    }

    // Stands in for librdkafka's negotiable range, which is the host's OpenSSL policy, not a constant.
    #[test]
    fn an_explicit_host_range_is_enforced_for_librdkafka() {
        // A host `MinProtocol = TLSv1.2` already guarantees the configured minimum.
        let host_floor_at_tls12 = (TlsVersion::Tls12, TlsVersion::Tls13);
        enforce_protocol_version_bounds(
            Some(TlsVersion::Tls12),
            None,
            "librdkafka",
            host_floor_at_tls12,
        )
        .unwrap();

        // An unrestricted host could still negotiate below the configured minimum.
        let unrestricted_host = (TlsVersion::Tls10, TlsVersion::Tls13);
        let error = enforce_protocol_version_bounds(
            Some(TlsVersion::Tls13),
            None,
            "librdkafka",
            unrestricted_host,
        )
        .expect_err("a host policy that cannot reach the configured minimum should be rejected");
        assert!(matches!(
            error,
            TlsError::ProtocolVersionUnenforceable { .. }
        ));
    }
}
