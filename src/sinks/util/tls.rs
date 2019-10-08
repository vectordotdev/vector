use native_tls::{Certificate, Identity, TlsConnectorBuilder};
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
use std::path::{Path, PathBuf};

#[derive(Debug, Snafu)]
enum TlsError {
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
    #[snafu(display("Could not set TCP TLS identity: {}", source))]
    TlsIdentityError { source: native_tls::Error },
    #[snafu(display("Could not export identity to DER: {}", source))]
    DerExportError { source: openssl::error::ErrorStack },
    #[snafu(display("Could not parse certificate in {:?}: {}", filename, source))]
    CertificateParseError {
        filename: PathBuf,
        source: native_tls::Error,
    },
    #[snafu(display("Must specify both TLS key_file and crt_file"))]
    MissingCrtKeyFile,
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

/// Standard TLS connector options
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
    accept_invalid_certificates: bool,
    accept_invalid_hostnames: bool,
    authority: Option<Certificate>,
    identity: Option<IdentityStore>, // native_tls::Identity doesn't implement Clone yet
}

#[derive(Clone)]
pub struct IdentityStore(Vec<u8>);

impl TlsSettings {
    pub fn from_options(options: &Option<TlsOptions>) -> crate::Result<Self> {
        let default = TlsOptions::default();
        let options = options.as_ref().unwrap_or(&default);

        if options.verify_certificate == Some(false) {
            warn!("`verify_certificate` is DISABLED, this may lead to security vulnerabilities");
        }
        if options.verify_hostname == Some(false) {
            warn!("`verify_hostname` is DISABLED, this may lead to security vulnerabilities");
        }

        if options.crt_path.is_some() != options.key_path.is_some() {
            return Err(TlsError::MissingCrtKeyFile.into());
        }

        let authority = match options.ca_path {
            None => None,
            Some(ref path) => Some(load_certificate(path)?),
        };

        let identity = match options.crt_path {
            None => None,
            Some(ref crt_path) => {
                let name = crt_path.to_string_lossy();
                let crt = load_x509(crt_path)?;
                let key_path = options.key_path.as_ref().unwrap();
                let key = load_key(&key_path, &options.key_pass)?;
                let pkcs12 = Pkcs12::builder()
                    .build("", &name, &key, &crt)
                    .context(Pkcs12Error)?;
                let identity = pkcs12.to_der().context(DerExportError)?;

                // Build the resulting Identity, but don't store it, as
                // it cannot be cloned.  This is just for error
                // checking.
                let _identity = Identity::from_pkcs12(&identity, "").context(TlsIdentityError)?;

                Some(IdentityStore(identity))
            }
        };

        Ok(Self {
            accept_invalid_certificates: !options.verify_certificate.unwrap_or(true),
            accept_invalid_hostnames: !options.verify_hostname.unwrap_or(true),
            authority,
            identity,
        })
    }
}

pub trait TlsConnectorExt {
    fn use_tls_settings(&mut self, settings: TlsSettings) -> &mut Self;
}

impl TlsConnectorExt for TlsConnectorBuilder {
    fn use_tls_settings(&mut self, settings: TlsSettings) -> &mut Self {
        self.danger_accept_invalid_certs(settings.accept_invalid_certificates);
        self.danger_accept_invalid_hostnames(settings.accept_invalid_hostnames);
        if let Some(certificate) = settings.authority {
            self.add_root_certificate(certificate);
        }
        if let Some(identity) = settings.identity {
            // This data was test-built previously, so we can just use
            // it here and expect the results will not fail. This can
            // all be reworked when `native_tls::Identity` gains the
            // Clone impl.
            let identity =
                Identity::from_pkcs12(&identity.0, "").expect("Could not build identity");
            self.identity(identity);
        }
        self
    }
}

/// Load a `native_tls::Certificate` from a named file
fn load_certificate(filename: &Path) -> crate::Result<Certificate> {
    let data = open_read(filename, "certificate")?;
    Ok(Certificate::from_pem(&data).with_context(|| CertificateParseError { filename })?)
}

/// Load a private key from a named file
fn load_key(filename: &Path, pass_phrase: &Option<String>) -> crate::Result<PKey<Private>> {
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

/// Load an X.509 certificate from a named file
fn load_x509(filename: &Path) -> crate::Result<X509> {
    let data = open_read(filename, "certificate")?;
    Ok(X509::from_pem(&data).with_context(|| X509ParseError { filename })?)
}

fn open_read(filename: &Path, note: &'static str) -> crate::Result<Vec<u8>> {
    let mut text = Vec::<u8>::new();

    File::open(filename)
        .with_context(|| FileOpenFailed { note, filename })?
        .read_to_end(&mut text)
        .with_context(|| FileReadFailed { note, filename })?;

    Ok(text)
}
