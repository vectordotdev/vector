use super::{
    AddCertToStore, AddExtraChainCert, DerExportError, FileOpenFailed, FileReadFailed, MaybeTls,
    NewStoreBuilder, ParsePkcs12, Pkcs12Error, PrivateKeyParseError, Result, SetCertificate,
    SetPrivateKey, SetVerifyCert, TlsError, TlsIdentityError, X509ParseError,
};
use openssl::{
    pkcs12::{ParsedPkcs12, Pkcs12},
    pkey::{PKey, Private},
    ssl::{ConnectConfiguration, SslContextBuilder, SslVerifyMode},
    x509::{store::X509StoreBuilder, X509},
};
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::fmt::{self, Debug, Formatter};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

const PEM_START_MARKER: &str = "-----BEGIN ";

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
    #[serde(alias = "ca_path")]
    pub ca_file: Option<PathBuf>,
    #[serde(alias = "crt_path")]
    pub crt_file: Option<PathBuf>,
    #[serde(alias = "key_path")]
    pub key_file: Option<PathBuf>,
    pub key_pass: Option<String>,
}

/// Directly usable settings for TLS connectors
#[derive(Clone, Default)]
pub struct TlsSettings {
    verify_certificate: bool,
    pub(super) verify_hostname: bool,
    authorities: Vec<X509>,
    pub(super) identity: Option<IdentityStore>, // openssl::pkcs12::ParsedPkcs12 doesn't impl Clone yet
}

#[derive(Clone)]
pub struct IdentityStore(Vec<u8>, String);

impl TlsSettings {
    /// Generate a filled out settings struct from the given optional
    /// option set, interpreted as client options. If `options` is
    /// `None`, the result is set to defaults (ie empty).
    pub fn from_options(options: &Option<TlsOptions>) -> Result<Self> {
        Self::from_options_base(options, false)
    }

    pub(super) fn from_options_base(
        options: &Option<TlsOptions>,
        for_server: bool,
    ) -> Result<Self> {
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

        Ok(Self {
            verify_certificate: options.verify_certificate.unwrap_or(!for_server),
            verify_hostname: options.verify_hostname.unwrap_or(!for_server),
            authorities: options.load_authorities()?,
            identity: options.load_identity()?,
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

    pub(super) fn apply_context(&self, context: &mut SslContextBuilder) -> Result<()> {
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
        if self.authorities.len() > 0 {
            let mut store = X509StoreBuilder::new().context(NewStoreBuilder)?;
            for authority in &self.authorities {
                store.add_cert(authority.clone()).context(AddCertToStore)?;
            }
            context
                .set_verify_cert_store(store.build())
                .context(SetVerifyCert)?;
        } else {
            debug!("Fetching system root certs.");

            #[cfg(windows)]
            load_windows_certs(context).unwrap();

            #[cfg(target_os = "macos")]
            load_mac_certs(context).unwrap();
        }

        Ok(())
    }

    pub fn apply_connect_configuration(&self, connection: &mut ConnectConfiguration) {
        connection.set_verify_hostname(self.verify_hostname);
    }
}

impl TlsOptions {
    fn load_authorities(&self) -> Result<Vec<X509>> {
        match &self.ca_file {
            None => Ok(vec![]),
            Some(filename) => {
                let (data, filename) = open_read(filename, "certificate")?;
                der_or_pem(
                    data,
                    |der| X509::from_der(&der).map(|x509| vec![x509]),
                    |pem| {
                        pem.match_indices(PEM_START_MARKER)
                            .map(|(start, _)| X509::from_pem(pem[start..].as_bytes()))
                            .collect()
                    },
                )
                .with_context(|| X509ParseError { filename })
            }
        }
    }

    fn load_identity(&self) -> Result<Option<IdentityStore>> {
        match (&self.crt_file, &self.key_file) {
            (None, Some(_)) => Err(TlsError::MissingCrtKeyFile.into()),
            (None, None) => Ok(None),
            (Some(filename), _) => {
                let (data, filename) = open_read(filename, "certificate")?;
                der_or_pem(
                    data,
                    |der| self.parse_pkcs12_identity(der),
                    |pem| self.parse_pem_identity(pem, &filename),
                )
            }
        }
    }

    /// Parse identity from a PEM encoded certificate + key pair of files
    fn parse_pem_identity(&self, pem: String, crt_file: &PathBuf) -> Result<Option<IdentityStore>> {
        match &self.key_file {
            None => Err(TlsError::MissingKey),
            Some(key_file) => {
                let name = crt_file.to_string_lossy().to_string();
                let crt = X509::from_pem(pem.as_bytes())
                    .with_context(|| X509ParseError { filename: crt_file })?;
                let key = load_key(&key_file, &self.key_pass)?;
                let pkcs12 = Pkcs12::builder()
                    .build("", &name, &key, &crt)
                    .context(Pkcs12Error)?;
                let identity = pkcs12.to_der().context(DerExportError)?;

                // Build the resulting parsed PKCS#12 archive,
                // but don't store it, as it cannot be cloned.
                // This is just for error checking.
                pkcs12.parse("").context(TlsIdentityError)?;

                Ok(Some(IdentityStore(identity, "".into())))
            }
        }
    }

    /// Parse identity from a DER encoded PKCS#12 archive
    fn parse_pkcs12_identity(&self, der: Vec<u8>) -> Result<Option<IdentityStore>> {
        let pkcs12 = Pkcs12::from_der(&der).context(ParsePkcs12)?;
        // Verify password
        let key_pass = self.key_pass.as_ref().map(|s| s.as_str()).unwrap_or("");
        pkcs12.parse(&key_pass).context(ParsePkcs12)?;
        Ok(Some(IdentityStore(der, key_pass.to_string())))
    }
}

/// === System Specific Root Cert ===
///
/// Most of this code is borrowed from https://github.com/ctz/rustls-native-certs

/// Load the system default certs from `schannel` this should be in place
/// of openssl-probe on linux.
#[cfg(windows)]
fn load_windows_certs(builder: &mut SslContextBuilder) -> Result<()> {
    use super::Schannel;

    let mut store = X509StoreBuilder::new().context(NewStoreBuilder)?;

    let current_user_store =
        schannel::cert_store::CertStore::open_current_user("ROOT").context(Schannel)?;

    for cert in current_user_store.certs() {
        let cert = cert.to_der().to_vec();
        let cert = X509::from_der(&cert[..]).context(super::X509SystemParseError)?;
        store.add_cert(cert).context(AddCertToStore)?;
    }

    builder
        .set_verify_cert_store(store.build())
        .context(SetVerifyCert)?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn load_mac_certs(builder: &mut SslContextBuilder) -> Result<()> {
    use super::SecurityFramework;
    use security_framework::trust_settings::{Domain, TrustSettings, TrustSettingsForCertificate};
    use std::collections::HashMap;

    // The various domains are designed to interact like this:
    //
    // "Per-user Trust Settings override locally administered
    //  Trust Settings, which in turn override the System Trust
    //  Settings."
    //
    // So we collect the certificates in this order; as a map of
    // their DER encoding to what we'll do with them.  We don't
    // overwrite existing elements, which mean User settings
    // trump Admin trump System, as desired.

    let mut store = X509StoreBuilder::new().context(NewStoreBuilder)?;
    let mut all_certs = HashMap::new();

    for domain in &[Domain::User, Domain::Admin, Domain::System] {
        let ts = TrustSettings::new(*domain);

        for cert in ts.iter().context(SecurityFramework)? {
            // If there are no specific trust settings, the default
            // is to trust the certificate as a root cert.  Weird API but OK.
            // The docs say:
            //
            // "Note that an empty Trust Settings array means "always trust this cert,
            //  with a resulting kSecTrustSettingsResult of kSecTrustSettingsResultTrustRoot".
            let trusted = ts
                .tls_trust_settings_for_certificate(&cert)
                .context(SecurityFramework)?
                .unwrap_or(TrustSettingsForCertificate::TrustRoot);

            all_certs.entry(cert.to_der()).or_insert(trusted);
        }
    }

    for (cert, trusted) in all_certs {
        if matches!(
            trusted,
            TrustSettingsForCertificate::TrustRoot | TrustSettingsForCertificate::TrustAsRoot
        ) {
            let cert = X509::from_der(&cert[..]).context(super::X509SystemParseError)?;
            store.add_cert(cert).context(AddCertToStore)?;
        }
    }

    builder
        .set_verify_cert_store(store.build())
        .context(SetVerifyCert)?;

    Ok(())
}

impl Debug for TlsSettings {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("TlsSettings")
            .field("verify_certificate", &self.verify_certificate)
            .field("verify_hostname", &self.verify_hostname)
            .finish()
    }
}

pub type MaybeTlsSettings = MaybeTls<(), TlsSettings>;

impl MaybeTlsSettings {
    pub fn enable_client() -> Result<Self> {
        let tls = TlsSettings::from_options_base(&None, false)?;
        Ok(Self::Tls(tls))
    }

    /// Generate an optional settings struct from the given optional
    /// configuration reference. If `config` is `None`, TLS is
    /// disabled. The `for_server` parameter indicates the options
    /// should be interpreted as being for a TLS server, which requires
    /// an identity certificate and changes the certificate verification
    /// default to false.
    pub fn from_config(config: &Option<TlsConfig>, for_server: bool) -> Result<Self> {
        match config {
            None => Ok(Self::Raw(())), // No config, no TLS settings
            Some(config) => match config.enabled.unwrap_or(false) {
                false => Ok(Self::Raw(())), // Explicitly disabled, still no TLS settings
                true => {
                    let tls =
                        TlsSettings::from_options_base(&Some(config.options.clone()), for_server)?;
                    match (for_server, &tls.identity) {
                        // Servers require an identity certificate
                        (true, None) => Err(TlsError::MissingRequiredIdentity.into()),
                        _ => Ok(Self::Tls(tls)),
                    }
                }
            },
        }
    }
}

impl From<TlsSettings> for MaybeTlsSettings {
    fn from(tls: TlsSettings) -> Self {
        Self::Tls(tls)
    }
}

/// Load a private key from a named file
fn load_key(filename: &Path, pass_phrase: &Option<String>) -> Result<PKey<Private>> {
    let (data, filename) = open_read(filename, "key")?;
    match pass_phrase {
        None => der_or_pem(
            data,
            |der| PKey::private_key_from_der(&der),
            |pem| PKey::private_key_from_pem(pem.as_bytes()),
        )
        .with_context(|| PrivateKeyParseError { filename }),
        Some(phrase) => der_or_pem(
            data,
            |der| PKey::private_key_from_pkcs8_passphrase(&der, phrase.as_bytes()),
            |pem| PKey::private_key_from_pem_passphrase(pem.as_bytes(), phrase.as_bytes()),
        )
        .with_context(|| PrivateKeyParseError { filename }),
    }
}

/// Parse the data one way if it looks like a DER file, and the other if
/// it looks like a PEM file. For the content to be treated as PEM, it
/// must parse as valid UTF-8 and contain a PEM start marker.
fn der_or_pem<T>(data: Vec<u8>, der_fn: impl Fn(Vec<u8>) -> T, pem_fn: impl Fn(String) -> T) -> T {
    // None of these steps cause (re)allocations,
    // just parsing and type manipulation
    match String::from_utf8(data) {
        Ok(text) => match text.find(PEM_START_MARKER) {
            Some(_) => pem_fn(text),
            None => der_fn(text.into_bytes()),
        },
        Err(err) => der_fn(err.into_bytes()),
    }
}

/// Open the named file and read its entire contents into memory. If the
/// file "name" contains a PEM start marker, it is assumed to contain
/// inline data and is used directly instead of opening a file.
fn open_read(filename: &Path, note: &'static str) -> Result<(Vec<u8>, PathBuf)> {
    if let Some(filename) = filename.to_str() {
        if let Some(_) = filename.find(PEM_START_MARKER) {
            return Ok((Vec::from(filename), "inline text".into()));
        }
    }

    let mut text = Vec::<u8>::new();

    File::open(filename)
        .with_context(|| FileOpenFailed { note, filename })?
        .read_to_end(&mut text)
        .with_context(|| FileReadFailed { note, filename })?;

    Ok((text, filename.into()))
}

#[cfg(test)]
mod test {
    use super::*;

    const TEST_PKCS12_PATH: &str = "tests/data/localhost.p12";
    const TEST_PEM_CRT_PATH: &str = "tests/data/localhost.crt";
    const TEST_PEM_CRT_BYTES: &[u8] = include_bytes!("../../tests/data/localhost.crt");
    const TEST_PEM_KEY_PATH: &str = "tests/data/localhost.key";
    const TEST_PEM_KEY_BYTES: &[u8] = include_bytes!("../../tests/data/localhost.key");

    #[test]
    fn from_options_pkcs12() {
        let options = TlsOptions {
            crt_file: Some(TEST_PKCS12_PATH.into()),
            key_pass: Some("NOPASS".into()),
            ..Default::default()
        };
        let settings =
            TlsSettings::from_options(&Some(options)).expect("Failed to load PKCS#12 certificate");
        assert!(settings.identity.is_some());
        assert_eq!(settings.authorities.len(), 0);
    }

    #[test]
    fn from_options_pem() {
        let options = TlsOptions {
            crt_file: Some(TEST_PEM_CRT_PATH.into()),
            key_file: Some(TEST_PEM_KEY_PATH.into()),
            ..Default::default()
        };
        let settings =
            TlsSettings::from_options(&Some(options)).expect("Failed to load PEM certificate");
        assert!(settings.identity.is_some());
        assert_eq!(settings.authorities.len(), 0);
    }

    #[test]
    fn from_options_inline_pem() {
        let crt = String::from_utf8(TEST_PEM_CRT_BYTES.to_vec()).unwrap();
        let key = String::from_utf8(TEST_PEM_KEY_BYTES.to_vec()).unwrap();
        let options = TlsOptions {
            crt_file: Some(crt.into()),
            key_file: Some(key.into()),
            ..Default::default()
        };
        let settings =
            TlsSettings::from_options(&Some(options)).expect("Failed to load PEM certificate");
        assert!(settings.identity.is_some());
        assert_eq!(settings.authorities.len(), 0);
    }

    #[test]
    fn from_options_ca() {
        let options = TlsOptions {
            ca_file: Some("tests/data/Vector_CA.crt".into()),
            ..Default::default()
        };
        let settings = TlsSettings::from_options(&Some(options))
            .expect("Failed to load authority certificate");
        assert!(settings.identity.is_none());
        assert_eq!(settings.authorities.len(), 1);
    }

    #[test]
    fn from_options_inline_ca() {
        let ca =
            String::from_utf8(include_bytes!("../../tests/data/Vector_CA.crt").to_vec()).unwrap();
        let options = TlsOptions {
            ca_file: Some(ca.into()),
            ..Default::default()
        };
        let settings = TlsSettings::from_options(&Some(options))
            .expect("Failed to load authority certificate");
        assert!(settings.identity.is_none());
        assert_eq!(settings.authorities.len(), 1);
    }

    #[test]
    fn from_options_multi_ca() {
        let options = TlsOptions {
            ca_file: Some("tests/data/Multi_CA.crt".into()),
            ..Default::default()
        };
        let settings = TlsSettings::from_options(&Some(options))
            .expect("Failed to load authority certificate");
        assert!(settings.identity.is_none());
        assert_eq!(settings.authorities.len(), 2);
    }

    #[test]
    fn from_options_none() {
        let settings = TlsSettings::from_options(&None).expect("Failed to generate null settings");
        assert!(settings.identity.is_none());
        assert_eq!(settings.authorities.len(), 0);
    }

    #[test]
    fn from_options_bad_certificate() {
        let options = TlsOptions {
            key_file: Some(TEST_PEM_KEY_PATH.into()),
            ..Default::default()
        };
        let error = TlsSettings::from_options(&Some(options))
            .expect_err("from_options failed to check certificate");
        assert!(matches!(error, TlsError::MissingCrtKeyFile));

        let options = TlsOptions {
            crt_file: Some(TEST_PEM_CRT_PATH.into()),
            ..Default::default()
        };
        let _error = TlsSettings::from_options(&Some(options))
            .expect_err("from_options failed to check certificate");
        // Actual error is an ASN parse, doesn't really matter
    }

    #[test]
    fn from_config_none() {
        assert!(MaybeTlsSettings::from_config(&None, true).unwrap().is_raw());
        assert!(MaybeTlsSettings::from_config(&None, false)
            .unwrap()
            .is_raw());
    }

    #[test]
    fn from_config_not_enabled() {
        assert!(settings_from_config(None, false, false, true).is_raw());
        assert!(settings_from_config(None, false, false, false).is_raw());
        assert!(settings_from_config(Some(false), false, false, true).is_raw());
        assert!(settings_from_config(Some(false), false, false, false).is_raw());
    }

    #[test]
    fn from_config_fails_without_certificate() {
        let config = make_config(Some(true), false, false);
        let error = MaybeTlsSettings::from_config(&Some(config), true)
            .expect_err("from_config failed to check for a certificate");
        assert!(matches!(error, TlsError::MissingRequiredIdentity));
    }

    #[test]
    fn from_config_with_certificate() {
        let config = settings_from_config(Some(true), true, true, true);
        assert!(config.is_tls());
    }

    fn settings_from_config(
        enabled: Option<bool>,
        set_crt: bool,
        set_key: bool,
        for_server: bool,
    ) -> MaybeTlsSettings {
        let config = make_config(enabled, set_crt, set_key);
        MaybeTlsSettings::from_config(&Some(config), for_server)
            .expect("Failed to generate settings from config")
    }

    fn make_config(enabled: Option<bool>, set_crt: bool, set_key: bool) -> TlsConfig {
        TlsConfig {
            enabled,
            options: TlsOptions {
                crt_file: and_some(set_crt, TEST_PEM_CRT_PATH.into()),
                key_file: and_some(set_key, TEST_PEM_KEY_PATH.into()),
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
