use native_tls::Certificate;
use openssl::{
    pkcs12::Pkcs12,
    pkey::{PKey, Private},
    x509::X509,
};
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

fn open_read<F: AsRef<Path> + Debug>(filename: F, note: &'static str) -> crate::Result<Vec<u8>> {
    let mut text = Vec::<u8>::new();
    let filename = filename.as_ref();

    File::open(filename)
        .with_context(|| FileOpenFailed { note, filename })?
        .read_to_end(&mut text)
        .with_context(|| FileReadFailed { note, filename })?;

    Ok(text)
}
