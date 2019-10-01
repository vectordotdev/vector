use native_tls::{Certificate, Identity, TlsConnectorBuilder};
use openssl::{
    pkcs12::Pkcs12,
    pkey::{PKey, Private},
    x509::X509,
};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::convert::TryFrom;
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
    pub ca_path: Option<PathBuf>,
    pub crt_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub key_pass: Option<String>,
}

impl TlsOptions {
    pub fn check_warnings(&self, sink: &str) {
        if self.verify_certificate == Some(false) {
            warn!(
                "`verify_certificate` in {} sink is DISABLED, this may lead to security vulnerabilities", sink
            );
        }
    }
}

/// Directly usable settings for TLS connectors
pub struct TlsSettings {
    verify_certificate: bool,
    authority: Option<Certificate>,
    identity: Option<Identity>,
}

impl TryFrom<&TlsOptions> for TlsSettings {
    type Error = crate::Error;
    fn try_from(options: &TlsOptions) -> Result<Self, Self::Error> {
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
                let identity = load_build_pkcs12(
                    options.key_path.as_ref().unwrap(),
                    &options.key_pass,
                    crt_path,
                )?;
                Some(
                    Identity::from_pkcs12(&identity.to_der().context(DerExportError)?, "")
                        .context(TlsIdentityError)?,
                )
            }
        };

        Ok(Self {
            verify_certificate: options.verify_certificate.unwrap_or(true),
            authority,
            identity,
        })
    }
}

impl TryFrom<TlsOptions> for TlsSettings {
    type Error = crate::Error;
    fn try_from(options: TlsOptions) -> Result<Self, Self::Error> {
        Self::try_from(&options)
    }
}

pub trait TlsConnectorExt {
    fn use_tls_settings(&mut self, settings: TlsSettings) -> &mut Self;
}

impl TlsConnectorExt for TlsConnectorBuilder {
    fn use_tls_settings(&mut self, settings: TlsSettings) -> &mut Self {
        self.danger_accept_invalid_certs(!settings.verify_certificate);
        if let Some(certificate) = settings.authority {
            self.add_root_certificate(certificate);
        }
        if let Some(identity) = settings.identity {
            self.identity(identity);
        }
        self
    }
}

/// Load a `native_tls::Certificate` from a named file
pub fn load_certificate<T: AsRef<Path> + Debug>(filename: T) -> crate::Result<Certificate> {
    let filename = filename.as_ref();
    let data = open_read(filename, "certificate")?;
    Ok(Certificate::from_pem(&data).with_context(|| CertificateParseError { filename })?)
}

/// Load a private key from a named file
pub fn load_key<T: AsRef<Path> + Debug>(
    filename: T,
    pass_phrase: &Option<String>,
) -> crate::Result<PKey<Private>> {
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

/// Load an X.509 certificate from a named file
pub fn load_x509<T: AsRef<Path> + Debug>(filename: T) -> crate::Result<X509> {
    let filename = filename.as_ref();
    let data = open_read(filename, "certificate")?;
    Ok(X509::from_pem(&data).with_context(|| X509ParseError { filename })?)
}

/// Load a key and certificate from a pair of files and build a PKCS#12 archive from them
pub fn load_build_pkcs12<P1, P2>(
    key_path: P1,
    key_pass: &Option<String>,
    crt_path: P2,
) -> crate::Result<Pkcs12>
where
    P1: AsRef<Path> + Debug,
    P2: AsRef<Path> + Debug,
{
    let crt_name = crt_path.as_ref().to_string_lossy();
    let key = load_key(key_path, key_pass)?;
    let crt = load_x509(crt_path.as_ref())?;
    Ok(Pkcs12::builder()
        .build("", &crt_name, &key, &crt)
        .context(Pkcs12Error)?)
}

pub fn open_read<F: AsRef<Path> + Debug>(
    filename: F,
    note: &'static str,
) -> crate::Result<Vec<u8>> {
    let mut text = Vec::<u8>::new();
    let filename = filename.as_ref();

    File::open(filename)
        .with_context(|| FileOpenFailed { note, filename })?
        .read_to_end(&mut text)
        .with_context(|| FileReadFailed { note, filename })?;

    Ok(text)
}
