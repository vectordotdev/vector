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
    pub(super) verify_hostname: bool,
    authority: Option<X509>,
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
                        pkcs12.parse(&key_pass).context(ParsePkcs12)?;
                        Some(IdentityStore(cert_data, key_pass.to_string()))
                    }
                    Err(source) => {
                        if options.key_path.is_none() {
                            return Err(TlsError::ParsePkcs12 { source });
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
        if let Some(certificate) = &self.authority {
            let mut store = X509StoreBuilder::new().context(NewStoreBuilder)?;
            store
                .add_cert(certificate.clone())
                .context(AddCertToStore)?;
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
fn load_x509(filename: &Path) -> Result<X509> {
    let data = open_read(filename, "certificate")?;
    Ok(X509::from_der(&data)
        .or_else(|_| X509::from_pem(&data))
        .with_context(|| X509ParseError { filename })?)
}

fn open_read(filename: &Path, note: &'static str) -> Result<Vec<u8>> {
    let mut text = Vec::<u8>::new();

    File::open(filename)
        .with_context(|| FileOpenFailed { note, filename })?
        .read_to_end(&mut text)
        .with_context(|| FileReadFailed { note, filename })?;

    Ok(text)
}

#[cfg(test)]
mod test {
    use super::*;

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
        assert!(matches!(error, TlsError::MissingCrtKeyFile));

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
