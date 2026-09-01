use std::{
    collections::HashMap,
    fmt,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

use super::{
    AddCertToStoreSnafu, AddExtraChainCertSnafu, CaStackPushSnafu, EncodeAlpnProtocolsSnafu,
    FileOpenFailedSnafu, FileReadFailedSnafu, MaybeTls, NewCaStackSnafu, NewStoreBuilderSnafu,
    ParsePkcs12Snafu, PrivateKeyParseSnafu, Result, SetAlpnProtocolsSnafu, SetCertificateSnafu,
    SetPrivateKeySnafu, SetTlsVersionSnafu, SetVerifyCertSnafu, TlsError, X509ParseSnafu,
};
use cfg_if::cfg_if;
use lookup::lookup_v2::OptionalValuePath;
use openssl::{
    pkcs12::Pkcs12,
    pkey::{PKey, Private},
    ssl::{
        AlpnError, ConnectConfiguration, SslContextBuilder, SslOptions, SslVerifyMode, SslVersion,
        select_next_proto,
    },
    stack::Stack,
    x509::{X509, store::X509StoreBuilder, verify::X509CheckFlags},
};
use snafu::ResultExt;
use vector_config::configurable_component;

pub const PEM_START_MARKER: &str = "-----BEGIN ";

pub const TEST_PEM_CA_PATH: &str = "tests/data/ca/certs/ca.cert.pem";
pub const TEST_PEM_INTERMEDIATE_CA_PATH: &str =
    "tests/data/ca/intermediate_server/certs/ca-chain.cert.pem";
pub const TEST_PEM_CRT_PATH: &str =
    "tests/data/ca/intermediate_server/certs/localhost-chain.cert.pem";
pub const TEST_PEM_KEY_PATH: &str = "tests/data/ca/intermediate_server/private/localhost.key.pem";
pub const TEST_PEM_CLIENT_CRT_PATH: &str =
    "tests/data/ca/intermediate_client/certs/localhost-chain.cert.pem";
pub const TEST_PEM_CLIENT_KEY_PATH: &str =
    "tests/data/ca/intermediate_client/private/localhost.key.pem";

/// Configures the TLS options for incoming/outgoing connections.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct TlsEnableableConfig {
    /// Whether to require TLS for incoming or outgoing connections.
    ///
    /// When enabled and used for incoming connections, an identity certificate is also required. See `tls.crt_file` for
    /// more information.
    pub enabled: Option<bool>,

    #[serde(flatten)]
    pub options: TlsConfig,
}

impl TlsEnableableConfig {
    pub fn enabled() -> Self {
        Self {
            enabled: Some(true),
            ..Self::default()
        }
    }

    pub fn test_config() -> Self {
        Self {
            enabled: Some(true),
            options: TlsConfig::test_config(),
        }
    }
}

/// `TlsEnableableConfig` for `sources`, adding metadata from the client certificate.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct TlsSourceConfig {
    /// Event field for client certificate metadata.
    pub client_metadata_key: Option<OptionalValuePath>,

    #[serde(flatten)]
    pub tls_config: TlsEnableableConfig,
}

/// TLS protocol version.
#[configurable_component]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TlsVersion {
    /// TLS v1.0.
    ///
    /// Deprecated by [RFC 8996][rfc_8996]. Only select this to interoperate with legacy peers
    /// that cannot be upgraded.
    ///
    /// [rfc_8996]: https://datatracker.ietf.org/doc/html/rfc8996
    #[serde(rename = "TLSv1")]
    Tls10,

    /// TLS v1.1.
    ///
    /// Deprecated by [RFC 8996][rfc_8996]. Only select this to interoperate with legacy peers
    /// that cannot be upgraded.
    ///
    /// [rfc_8996]: https://datatracker.ietf.org/doc/html/rfc8996
    #[serde(rename = "TLSv1.1")]
    Tls11,

    /// TLS v1.2.
    #[serde(rename = "TLSv1.2")]
    Tls12,

    /// TLS v1.3.
    #[serde(rename = "TLSv1.3")]
    Tls13,
}

impl TlsVersion {
    const fn as_ssl_version(self) -> SslVersion {
        match self {
            Self::Tls10 => SslVersion::TLS1,
            Self::Tls11 => SslVersion::TLS1_1,
            Self::Tls12 => SslVersion::TLS1_2,
            Self::Tls13 => SslVersion::TLS1_3,
        }
    }
}

impl fmt::Display for TlsVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Tls10 => "TLSv1",
            Self::Tls11 => "TLSv1.1",
            Self::Tls12 => "TLSv1.2",
            Self::Tls13 => "TLSv1.3",
        })
    }
}

/// TLS configuration.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    /// Enables certificate verification. For components that create a server, this requires that the
    /// client connections have a valid client certificate. For components that initiate requests,
    /// this validates that the upstream has a valid certificate.
    ///
    /// If enabled, certificates must not be expired and must be issued by a trusted
    /// issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
    /// certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
    /// so on, until the verification process reaches a root certificate.
    ///
    /// Do NOT set this to `false` unless you understand the risks of not verifying the validity of certificates.
    pub verify_certificate: Option<bool>,

    /// Enables hostname verification.
    ///
    /// If enabled, the hostname used to connect to the remote host must be present in the TLS certificate presented by
    /// the remote host, either as the Common Name or as an entry in the Subject Alternative Name extension.
    ///
    /// Only relevant for outgoing connections.
    ///
    /// Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname.
    pub verify_hostname: Option<bool>,

    /// Sets the list of supported ALPN protocols.
    ///
    /// Declare the supported ALPN protocols, which are used during negotiation with a peer. They are prioritized in the order
    /// that they are defined.
    #[configurable(metadata(docs::examples = "h2"))]
    pub alpn_protocols: Option<Vec<String>>,

    /// Absolute path to an additional CA certificate file.
    ///
    /// The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
    #[serde(alias = "ca_path")]
    #[configurable(metadata(docs::examples = "/path/to/certificate_authority.crt"))]
    #[configurable(metadata(docs::human_name = "CA File Path"))]
    pub ca_file: Option<PathBuf>,

    /// Absolute path to a certificate file used to identify this server.
    ///
    /// The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
    /// an inline string in PEM format.
    ///
    /// If this is set _and_ is not a PKCS#12 archive, `key_file` must also be set.
    #[serde(alias = "crt_path")]
    #[configurable(metadata(docs::examples = "/path/to/host_certificate.crt"))]
    #[configurable(metadata(docs::human_name = "Certificate File Path"))]
    pub crt_file: Option<PathBuf>,

    /// Absolute path to a private key file used to identify this server.
    ///
    /// The key must be in DER or PEM (PKCS#8) format. Additionally, the key can be provided as an inline string in PEM format.
    #[serde(alias = "key_path")]
    #[configurable(metadata(docs::examples = "/path/to/host_certificate.key"))]
    #[configurable(metadata(docs::human_name = "Key File Path"))]
    pub key_file: Option<PathBuf>,

    /// Passphrase used to unlock the encrypted key file.
    ///
    /// This has no effect unless `key_file` is set.
    #[configurable(metadata(docs::examples = "${KEY_PASS_ENV_VAR}"))]
    #[configurable(metadata(docs::examples = "PassWord1"))]
    #[configurable(metadata(docs::human_name = "Key File Password"))]
    pub key_pass: Option<String>,

    /// Server name to use when using Server Name Indication (SNI).
    ///
    /// Only relevant for outgoing connections.
    #[serde(alias = "server_name")]
    #[configurable(metadata(docs::examples = "www.example.com"))]
    #[configurable(metadata(docs::human_name = "Server Name"))]
    pub server_name: Option<String>,

    /// Minimum TLS protocol version to negotiate.
    ///
    /// Peers that cannot negotiate at least this version are rejected during the handshake.
    ///
    /// When unset, the minimum is whatever the underlying TLS library permits, which currently
    /// includes the deprecated TLS v1.0 and v1.1. Set this to `TLSv1.2` to refuse them.
    ///
    /// Components that accept connections do not offer TLS v1.3 by default. Setting either this
    /// option or `max_tls_version` enables every version within the resulting window, so
    /// `min_tls_version: TLSv1.2` also makes TLS v1.3 available.
    #[configurable(metadata(docs::human_name = "Minimum TLS Version"))]
    pub min_tls_version: Option<TlsVersion>,

    /// Maximum TLS protocol version to negotiate.
    ///
    /// Peers are never offered a version newer than this. This is rarely needed, and is intended
    /// for working around peers that advertise support for a version they cannot actually
    /// negotiate.
    ///
    /// When unset, the maximum is whatever the underlying TLS library permits. Note that for
    /// components that accept connections, TLS v1.3 is disabled unless either this option or
    /// `min_tls_version` is set.
    #[configurable(metadata(docs::human_name = "Maximum TLS Version"))]
    pub max_tls_version: Option<TlsVersion>,
}

impl TlsConfig {
    /// Whether an explicit TLS protocol version window was configured.
    ///
    /// Components that hand this configuration to a third-party TLS stack rather than applying it
    /// to an OpenSSL context use this to warn instead of ignoring the bounds silently. See
    /// [`warn_unenforceable_protocol_versions`](super::warn_unenforceable_protocol_versions).
    pub fn has_protocol_version_bounds(&self) -> bool {
        self.min_tls_version.is_some() || self.max_tls_version.is_some()
    }

    pub fn test_config() -> Self {
        Self {
            ca_file: Some(TEST_PEM_CA_PATH.into()),
            crt_file: Some(TEST_PEM_CRT_PATH.into()),
            key_file: Some(TEST_PEM_KEY_PATH.into()),
            ..Self::default()
        }
    }
}

/// Directly usable settings for TLS connectors
#[derive(Clone, Default)]
pub struct TlsSettings {
    verify_certificate: bool,
    pub(super) verify_hostname: bool,
    authorities: Vec<X509>,
    pub(super) identity: Option<IdentityStore>,
    alpn_protocols: Option<Vec<u8>>,
    server_name: Option<String>,
    min_tls_version: Option<TlsVersion>,
    max_tls_version: Option<TlsVersion>,
}

/// Identity store in PEM format
#[derive(Clone)]
pub(super) struct IdentityStore {
    cert: X509,
    key: PKey<Private>,
    ca: Option<Vec<X509>>,
}

impl TlsSettings {
    /// Generate a filled out settings struct from the given optional
    /// option set, interpreted as client options. If `options` is
    /// `None`, the result is set to defaults (ie empty).
    pub fn from_options(options: Option<&TlsConfig>) -> Result<Self> {
        Self::from_options_base(options, false)
    }

    pub(super) fn from_options_base(options: Option<&TlsConfig>, for_server: bool) -> Result<Self> {
        let default = TlsConfig::default();
        let options = options.unwrap_or(&default);

        if !for_server {
            if options.verify_certificate == Some(false) {
                warn!(
                    "The `verify_certificate` option is DISABLED, this may lead to security vulnerabilities."
                );
            }
            if options.verify_hostname == Some(false) {
                warn!(
                    "The `verify_hostname` option is DISABLED, this may lead to security vulnerabilities."
                );
            }
        }

        if let (Some(min), Some(max)) = (options.min_tls_version, options.max_tls_version)
            && min > max
        {
            return Err(TlsError::InvalidTlsVersionRange { min, max });
        }

        Ok(Self {
            verify_certificate: options.verify_certificate.unwrap_or(!for_server),
            verify_hostname: options.verify_hostname.unwrap_or(!for_server),
            authorities: options.load_authorities()?,
            identity: options.load_identity()?,
            alpn_protocols: options.parse_alpn_protocols()?,
            server_name: options.server_name.clone(),
            min_tls_version: options.min_tls_version,
            max_tls_version: options.max_tls_version,
        })
    }

    /// The configured SNI server name override, if any.
    pub fn server_name(&self) -> Option<&str> {
        self.server_name.as_deref()
    }

    /// Whether certificate hostname verification is enabled.
    pub fn verify_hostname(&self) -> bool {
        self.verify_hostname
    }

    /// Whether an explicit TLS protocol version window was configured.
    ///
    /// Components that hand the PEM material to a third-party TLS stack instead of applying
    /// these settings to an OpenSSL context cannot honor `min_tls_version`/`max_tls_version`,
    /// and use this to warn rather than ignore them silently. See
    /// [`warn_unenforceable_protocol_versions`](super::warn_unenforceable_protocol_versions).
    pub fn has_protocol_version_bounds(&self) -> bool {
        self.min_tls_version.is_some() || self.max_tls_version.is_some()
    }

    /// Returns the identity as PEM encoded byte arrays
    ///
    /// # Panics
    ///
    /// Panics if the identity is missing, invalid, or the authorities to chain are invalid.
    pub fn identity_pem(&self) -> Option<(Vec<u8>, Vec<u8>)> {
        self.identity.as_ref().map(|identity| {
            // we have verified correct formatting at ingest time
            let mut cert = identity.cert.to_pem().expect("Invalid stored identity");
            let key = identity
                .key
                .private_key_to_pem_pkcs8()
                .expect("Invalid stored identity");
            if let Some(chain) = identity.ca.as_ref() {
                for authority in chain {
                    cert.extend(
                        authority
                            .to_pem()
                            .expect("Invalid stored identity chain certificate"),
                    );
                }
            }
            (cert, key)
        })
    }

    /// Returns the authorities as PEM data
    ///
    /// # Panics
    ///
    /// Panics if the authority is invalid.
    pub fn authorities_pem(&self) -> impl Iterator<Item = Vec<u8>> + '_ {
        self.authorities.iter().map(|authority| {
            authority
                .to_pem()
                .expect("Invalid stored authority certificate")
        })
    }

    pub(super) fn apply_context(&self, context: &mut SslContextBuilder) -> Result<()> {
        self.apply_context_base(context, false)
    }

    /// Constrains `context` to the configured `[min_tls_version, max_tls_version]` window.
    ///
    /// Does nothing unless at least one bound is configured, so the library defaults are left
    /// untouched for anyone who has not opted in.
    fn apply_protocol_versions(&self, context: &mut SslContextBuilder) -> Result<()> {
        if self.min_tls_version.is_none() && self.max_tls_version.is_none() {
            return Ok(());
        }

        // Each setter is called only when Vector has an explicit bound for that side.
        // `SSL_CTX_set_min_proto_version(0)` -- which is what passing `None` compiles to --
        // does not mean "leave unchanged", it clears whatever bound is already in force. That
        // includes a bound applied from the host's OpenSSL configuration (`MinProtocol` in
        // `openssl.cnf`), so unconditionally calling both setters would let a config that sets
        // only one side silently re-enable versions the host policy forbids.
        if let Some(min) = self.min_tls_version {
            context
                .set_min_proto_version(Some(min.as_ssl_version()))
                .context(SetTlsVersionSnafu)?;
        }
        if let Some(max) = self.max_tls_version {
            context
                .set_max_proto_version(Some(max.as_ssl_version()))
                .context(SetTlsVersionSnafu)?;
        }

        // Acceptors are built from `SslAcceptor::mozilla_intermediate`, which sets
        // `SSL_OP_NO_TLSv1_3`. OpenSSL treats the `SSL_OP_NO_*` options as a veto that outranks
        // the min/max protocol version, so without clearing it a window containing TLS v1.3
        // would still exclude v1.3 -- and a window of v1.3 alone would leave no usable version.
        //
        // Only this one option is cleared. Vector never sets the other `SSL_OP_NO_*` version
        // flags, so clearing them could only relax a restriction configured elsewhere.
        if self.window_contains(TlsVersion::Tls13) {
            context.clear_options(SslOptions::NO_TLSV1_3);
        }

        Ok(())
    }

    /// Whether `version` falls inside the configured `[min_tls_version, max_tls_version]` window.
    ///
    /// An unset bound is unbounded on that side.
    fn window_contains(&self, version: TlsVersion) -> bool {
        self.min_tls_version.is_none_or(|min| version >= min)
            && self.max_tls_version.is_none_or(|max| version <= max)
    }

    pub(super) fn apply_context_base(
        &self,
        context: &mut SslContextBuilder,
        for_server: bool,
    ) -> Result<()> {
        self.apply_protocol_versions(context)?;

        context.set_verify(if self.verify_certificate {
            SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT
        } else {
            SslVerifyMode::NONE
        });
        if let Some(identity) = &self.identity {
            context
                .set_certificate(&identity.cert)
                .context(SetCertificateSnafu)?;
            context
                .set_private_key(&identity.key)
                .context(SetPrivateKeySnafu)?;

            if let Some(chain) = &identity.ca {
                for cert in chain {
                    context
                        .add_extra_chain_cert(cert.clone())
                        .context(AddExtraChainCertSnafu)?;
                }
            }
        }
        if self.authorities.is_empty() {
            debug!("Fetching system root certs.");

            cfg_if! {
                if #[cfg(windows)] {
                    load_windows_certs(context).unwrap();
                } else if #[cfg(target_os = "macos")] {
                    cfg_if! { // Panic in release builds, warn in debug builds.
                        if #[cfg(debug_assertions)] {
                            if let Err(error) = load_mac_certs(context) {
                                warn!("Failed to load macOS certs: {error}");
                            }
                        } else {
                            load_mac_certs(context).unwrap();
                        }
                    }
                }
            }
        } else {
            let mut store = X509StoreBuilder::new().context(NewStoreBuilderSnafu)?;
            for authority in &self.authorities {
                store
                    .add_cert(authority.clone())
                    .context(AddCertToStoreSnafu)?;
            }
            context
                .set_verify_cert_store(store.build())
                .context(SetVerifyCertSnafu)?;
        }

        if let Some(alpn) = &self.alpn_protocols {
            if for_server {
                // The server ALPN select callback requires a `'static` protocol list (see
                // https://github.com/sfackler/rust-openssl/pull/2360). Intern it so the intentional
                // leak happens at most once per distinct list, rather than leaking a fresh copy
                // every time the context is (re)built.
                let server_proto_ref = intern_alpn_protocols(alpn);
                context.set_alpn_select_callback(move |_, client_proto| {
                    select_next_proto(server_proto_ref, client_proto).ok_or(AlpnError::NOACK)
                });
            } else {
                context
                    .set_alpn_protos(alpn.as_slice())
                    .context(SetAlpnProtocolsSnafu)?;
            }
        }

        Ok(())
    }

    /// Apply per-connection TLS settings.
    ///
    /// `skip_server_name` must be set when the connection targets a forward proxy rather than the
    /// upstream destination. The `server_name` override applies only to the destination; applying it
    /// to the proxy's own TLS connection would verify the proxy certificate against the upstream
    /// name and fail.
    pub fn apply_connect_configuration(
        &self,
        connection: &mut ConnectConfiguration,
        skip_server_name: bool,
    ) -> std::result::Result<(), openssl::error::ErrorStack> {
        if let Some(server_name) = self.server_name.as_deref().filter(|_| !skip_server_name) {
            // Use the configured server name for both SNI and certificate hostname
            // verification. `ConnectConfiguration::into_ssl` (called by the connector
            // after this callback) would otherwise apply the URL host to SNI and the
            // verify parameter, overriding the configured server name and causing a
            // hostname mismatch. Disabling both here prevents that override.
            connection.set_use_server_name_indication(false);
            connection.set_verify_hostname(false);

            let server_ip = server_name.parse::<std::net::IpAddr>();

            // SNI must be a hostname, not an IP literal.
            if server_ip.is_err() {
                connection.set_hostname(server_name)?;
            }

            if self.verify_hostname {
                // Mirror `ConnectConfiguration::into_ssl`'s `setup_verify_hostname` so that
                // verification against `server_name` behaves exactly as it would against the
                // URL host, just with our name instead:
                // https://github.com/rust-openssl/rust-openssl/blob/db9c9e2f5db2ad7b45fd894e8d297ee15bfd0c7c/openssl/src/ssl/connector.rs#L380-L389
                let param = connection.param_mut();
                // Disallow partial-wildcard matches such as `w*.example.com` matching
                // `www.example.com`, so a wildcard label must be the entire leftmost label
                // (`*.example.com`).
                param.set_hostflags(X509CheckFlags::NO_PARTIAL_WILDCARDS);
                match server_ip {
                    Ok(ip) => param.set_ip(ip)?,
                    Err(_) => param.set_host(server_name)?,
                }
            }
        } else {
            connection.set_verify_hostname(self.verify_hostname);
        }
        Ok(())
    }
}

/// Return a `'static` copy of a server ALPN protocol list, leaking each distinct list at most once.
///
/// `SslContextBuilder::set_alpn_select_callback` requires the protocol list to outlive the context
/// with a `'static` lifetime, so the bytes must be leaked. Interning by content means rebuilding an
/// acceptor with the same ALPN configuration (e.g. on every certificate reload) reuses the existing
/// allocation instead of leaking a fresh copy each time, keeping the leak bounded and one-time.
fn intern_alpn_protocols(protocols: &[u8]) -> &'static [u8] {
    static INTERNED: LazyLock<Mutex<HashMap<Vec<u8>, &'static [u8]>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let mut interned = INTERNED.lock().expect("mutex poisoned");

    if let Some(existing) = interned.get(protocols).copied() {
        return existing;
    }
    let leaked: &'static [u8] = Box::leak(protocols.to_vec().into_boxed_slice());
    interned.insert(protocols.to_vec(), leaked);
    leaked
}

impl TlsConfig {
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
                            .map(|(start, _)| X509::from_pem(&pem.as_bytes()[start..]))
                            .collect()
                    },
                )
                .with_context(|_| X509ParseSnafu { filename })
            }
        }
    }

    fn load_identity(&self) -> Result<Option<IdentityStore>> {
        match (&self.crt_file, &self.key_file) {
            (None, Some(_)) => Err(TlsError::MissingCrtKeyFile),
            (None, None) => Ok(None),
            (Some(filename), _) => {
                let (data, filename) = open_read(filename, "certificate")?;
                der_or_pem(
                    data,
                    |der| self.parse_pkcs12_identity(&der),
                    |pem| self.parse_pem_identity(&pem, &filename),
                )
            }
        }
    }

    /// The input must be in ALPN "wire format".
    ///
    /// It consists of a sequence of supported protocol names prefixed by their byte length.
    fn parse_alpn_protocols(&self) -> Result<Option<Vec<u8>>> {
        match &self.alpn_protocols {
            None => Ok(None),
            Some(protocols) => {
                let mut data: Vec<u8> = Vec::new();
                for str in protocols {
                    data.push(str.len().try_into().context(EncodeAlpnProtocolsSnafu)?);
                    data.append(&mut str.clone().into_bytes());
                }
                Ok(Some(data))
            }
        }
    }

    /// Parse identity from a PEM encoded certificate + key pair of files
    fn parse_pem_identity(&self, pem: &str, crt_file: &Path) -> Result<Option<IdentityStore>> {
        match &self.key_file {
            None => Err(TlsError::MissingKey),
            Some(key_file) => {
                let mut crt_stack = X509::stack_from_pem(pem.as_bytes())
                    .with_context(|_| X509ParseSnafu { filename: crt_file })?
                    .into_iter();

                let cert = crt_stack.next().ok_or(TlsError::MissingCertificate)?;
                let key = load_key(key_file.as_path(), self.key_pass.as_ref())?;

                let mut ca_stack = Stack::new().context(NewCaStackSnafu)?;
                for intermediate in crt_stack {
                    ca_stack.push(intermediate).context(CaStackPushSnafu)?;
                }
                let ca: Vec<X509> = ca_stack
                    .iter()
                    .map(std::borrow::ToOwned::to_owned)
                    .collect();
                Ok(Some(IdentityStore {
                    cert,
                    key,
                    ca: Some(ca),
                }))
            }
        }
    }

    /// Parse identity from a DER encoded PKCS#12 archive
    fn parse_pkcs12_identity(&self, der: &[u8]) -> Result<Option<IdentityStore>> {
        let pkcs12 = Pkcs12::from_der(der).context(ParsePkcs12Snafu)?;
        // Verify password
        let key_pass = self.key_pass.as_deref().unwrap_or("");
        let parsed = pkcs12.parse2(key_pass).context(ParsePkcs12Snafu)?;
        // extract cert, key and ca and store as PEM sow e can return an IdentityStore
        let cert = parsed.cert.ok_or(TlsError::MissingCertificate)?;
        let key = parsed.pkey.ok_or(TlsError::MissingKey)?;
        let ca: Option<Vec<X509>> = parsed
            .ca
            .map(|stack| stack.iter().map(std::borrow::ToOwned::to_owned).collect());
        Ok(Some(IdentityStore { cert, key, ca }))
    }
}

/// === System Specific Root Cert ===
///
/// Most of this code is borrowed from https://github.com/ctz/rustls-native-certs

/// Load the system default certs from `schannel` this should be in place
/// of openssl-probe on linux.
#[cfg(windows)]
fn load_windows_certs(builder: &mut SslContextBuilder) -> Result<()> {
    use super::SchannelSnafu;

    let mut store = X509StoreBuilder::new().context(NewStoreBuilderSnafu)?;

    let current_user_store =
        schannel::cert_store::CertStore::open_current_user("ROOT").context(SchannelSnafu)?;

    for cert in current_user_store.certs() {
        let cert = cert.to_der().to_vec();
        let cert = X509::from_der(&cert[..]).context(super::X509SystemParseSnafu)?;
        store.add_cert(cert).context(AddCertToStoreSnafu)?;
    }

    builder
        .set_verify_cert_store(store.build())
        .context(SetVerifyCertSnafu)?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn load_mac_certs(builder: &mut SslContextBuilder) -> Result<()> {
    use std::collections::HashMap;

    use security_framework::trust_settings::{Domain, TrustSettings, TrustSettingsForCertificate};

    use super::SecurityFrameworkSnafu;

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

    let mut store = X509StoreBuilder::new().context(NewStoreBuilderSnafu)?;
    let mut all_certs = HashMap::new();

    for domain in &[Domain::User, Domain::Admin, Domain::System] {
        let ts = TrustSettings::new(*domain);

        for cert in ts.iter().context(SecurityFrameworkSnafu)? {
            // If there are no specific trust settings, the default
            // is to trust the certificate as a root cert.  Weird API but OK.
            // The docs say:
            //
            // "Note that an empty Trust Settings array means "always trust this cert,
            //  with a resulting kSecTrustSettingsResult of kSecTrustSettingsResultTrustRoot".
            let trusted = ts
                .tls_trust_settings_for_certificate(&cert)
                .context(SecurityFrameworkSnafu)?
                .unwrap_or(TrustSettingsForCertificate::TrustRoot);

            all_certs.entry(cert.to_der()).or_insert(trusted);
        }
    }

    for (cert, trusted) in all_certs {
        if matches!(
            trusted,
            TrustSettingsForCertificate::TrustRoot | TrustSettingsForCertificate::TrustAsRoot
        ) {
            let cert = X509::from_der(&cert[..]).context(super::X509SystemParseSnafu)?;
            store.add_cert(cert).context(AddCertToStoreSnafu)?;
        }
    }

    builder
        .set_verify_cert_store(store.build())
        .context(SetVerifyCertSnafu)?;

    Ok(())
}

impl fmt::Debug for TlsSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsSettings")
            .field("verify_certificate", &self.verify_certificate)
            .field("verify_hostname", &self.verify_hostname)
            .finish_non_exhaustive()
    }
}

pub type MaybeTlsSettings = MaybeTls<(), TlsSettings>;

impl MaybeTlsSettings {
    pub fn enable_client() -> Result<Self> {
        let tls = TlsSettings::from_options_base(None, false)?;
        Ok(Self::Tls(tls))
    }

    pub fn tls_client(config: Option<&TlsConfig>) -> Result<Self> {
        Ok(Self::Tls(TlsSettings::from_options_base(config, false)?))
    }

    /// Generate an optional settings struct from the given optional
    /// configuration reference. If `config` is `None`, TLS is
    /// disabled. The `for_server` parameter indicates the options
    /// should be interpreted as being for a TLS server, which requires
    /// an identity certificate and changes the certificate verification
    /// default to false.
    pub fn from_config(config: Option<&TlsEnableableConfig>, for_server: bool) -> Result<Self> {
        match config {
            None => Ok(Self::Raw(())), // No config, no TLS settings
            Some(config) => {
                if config.enabled.unwrap_or(false) {
                    let tls = TlsSettings::from_options_base(Some(&config.options), for_server)?;
                    match (for_server, &tls.identity) {
                        // Servers require an identity certificate
                        (true, None) => Err(TlsError::MissingRequiredIdentity),
                        _ => Ok(Self::Tls(tls)),
                    }
                } else {
                    Ok(Self::Raw(())) // Explicitly disabled, still no TLS settings
                }
            }
        }
    }

    pub const fn http_protocol_name(&self) -> &'static str {
        match self {
            MaybeTls::Raw(()) => "http",
            MaybeTls::Tls(_) => "https",
        }
    }
}

impl From<TlsSettings> for MaybeTlsSettings {
    fn from(tls: TlsSettings) -> Self {
        Self::Tls(tls)
    }
}

/// Load a private key from a named file
fn load_key(filename: &Path, pass_phrase: Option<&String>) -> Result<PKey<Private>> {
    let (data, filename) = open_read(filename, "key")?;
    match pass_phrase {
        None => der_or_pem(
            data,
            |der| PKey::private_key_from_der(&der),
            |pem| PKey::private_key_from_pem(pem.as_bytes()),
        )
        .with_context(|_| PrivateKeyParseSnafu { filename }),
        Some(phrase) => der_or_pem(
            data,
            |der| PKey::private_key_from_pkcs8_passphrase(&der, phrase.as_bytes()),
            |pem| PKey::private_key_from_pem_passphrase(pem.as_bytes(), phrase.as_bytes()),
        )
        .with_context(|_| PrivateKeyParseSnafu { filename }),
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
    if let Some(filename) = filename.to_str()
        && filename.contains(PEM_START_MARKER)
    {
        return Ok((Vec::from(filename), "inline text".into()));
    }

    let mut text = Vec::<u8>::new();

    File::open(filename)
        .with_context(|_| FileOpenFailedSnafu { note, filename })?
        .read_to_end(&mut text)
        .with_context(|_| FileReadFailedSnafu { note, filename })?;

    Ok((text, filename.into()))
}

#[cfg(test)]
mod test {
    use openssl::ssl::{SslAcceptor, SslConnector, SslMethod};

    use super::*;

    const TEST_PKCS12_PATH: &str = "tests/data/ca/intermediate_client/private/localhost.p12";
    const TEST_PEM_CRT_BYTES: &[u8] =
        include_bytes!("../../../../tests/data/ca/intermediate_server/certs/localhost.cert.pem");
    const TEST_PEM_KEY_BYTES: &[u8] =
        include_bytes!("../../../../tests/data/ca/intermediate_server/private/localhost.key.pem");

    #[test]
    fn parse_alpn_protocols() {
        let options = TlsConfig {
            alpn_protocols: Some(vec![String::from("h2")]),
            ..Default::default()
        };
        let settings =
            TlsSettings::from_options(Some(&options)).expect("Failed to parse alpn_protocols");
        assert_eq!(settings.alpn_protocols, Some(vec![2, 104, 50]));
    }

    #[test]
    fn from_options_pkcs12() {
        let _provider = openssl::provider::Provider::try_load(None, "legacy", true).unwrap();
        let options = TlsConfig {
            crt_file: Some(TEST_PKCS12_PATH.into()),
            key_pass: Some("NOPASS".into()),
            ..Default::default()
        };
        let settings =
            TlsSettings::from_options(Some(&options)).expect("Failed to load PKCS#12 certificate");
        assert!(settings.identity.is_some());
        assert_eq!(settings.authorities.len(), 0);
    }

    #[test]
    fn from_options_pem() {
        let options = TlsConfig {
            crt_file: Some(TEST_PEM_CRT_PATH.into()),
            key_file: Some(TEST_PEM_KEY_PATH.into()),
            ..Default::default()
        };
        let settings =
            TlsSettings::from_options(Some(&options)).expect("Failed to load PEM certificate");
        assert!(settings.identity.is_some());
        assert_eq!(settings.authorities.len(), 0);
    }

    #[test]
    fn from_options_inline_pem() {
        let crt = String::from_utf8(TEST_PEM_CRT_BYTES.to_vec()).unwrap();
        let key = String::from_utf8(TEST_PEM_KEY_BYTES.to_vec()).unwrap();
        let options = TlsConfig {
            crt_file: Some(crt.into()),
            key_file: Some(key.into()),
            ..Default::default()
        };
        let settings =
            TlsSettings::from_options(Some(&options)).expect("Failed to load PEM certificate");
        assert!(settings.identity.is_some());
        assert_eq!(settings.authorities.len(), 0);
    }

    #[test]
    fn from_options_ca() {
        let options = TlsConfig {
            ca_file: Some(TEST_PEM_CA_PATH.into()),
            ..Default::default()
        };
        let settings = TlsSettings::from_options(Some(&options))
            .expect("Failed to load authority certificate");
        assert!(settings.identity.is_none());
        assert_eq!(settings.authorities.len(), 1);
    }

    #[test]
    fn from_options_inline_ca() {
        let ca = String::from_utf8(
            include_bytes!("../../../../tests/data/ca/certs/ca.cert.pem").to_vec(),
        )
        .unwrap();
        let options = TlsConfig {
            ca_file: Some(ca.into()),
            ..Default::default()
        };
        let settings = TlsSettings::from_options(Some(&options))
            .expect("Failed to load authority certificate");
        assert!(settings.identity.is_none());
        assert_eq!(settings.authorities.len(), 1);
    }

    #[test]
    fn from_options_intermediate_ca() {
        let options = TlsConfig {
            ca_file: Some("tests/data/ca/intermediate_server/certs/ca-chain.cert.pem".into()),
            ..Default::default()
        };
        let settings = TlsSettings::from_options(Some(&options))
            .expect("Failed to load authority certificate");
        assert!(settings.identity.is_none());
        assert_eq!(settings.authorities.len(), 2);
    }

    #[test]
    fn from_options_multi_ca() {
        let options = TlsConfig {
            ca_file: Some("tests/data/Multi_CA.crt".into()),
            ..Default::default()
        };
        let settings = TlsSettings::from_options(Some(&options))
            .expect("Failed to load authority certificate");
        assert!(settings.identity.is_none());
        assert_eq!(settings.authorities.len(), 2);
    }

    #[test]
    fn from_options_none() {
        let settings = TlsSettings::from_options(None).expect("Failed to generate null settings");
        assert!(settings.identity.is_none());
        assert_eq!(settings.authorities.len(), 0);
    }

    #[test]
    fn from_options_bad_certificate() {
        let options = TlsConfig {
            key_file: Some(TEST_PEM_KEY_PATH.into()),
            ..Default::default()
        };
        let error = TlsSettings::from_options(Some(&options))
            .expect_err("from_options failed to check certificate");
        assert!(matches!(error, TlsError::MissingCrtKeyFile));

        let options = TlsConfig {
            crt_file: Some(TEST_PEM_CRT_PATH.into()),
            ..Default::default()
        };
        let _error = TlsSettings::from_options(Some(&options))
            .expect_err("from_options failed to check certificate");
        // Actual error is an ASN parse, doesn't really matter
    }

    // End-to-end regression test for the OpenSSL hostname-mismatch bug: the server presents a
    // certificate for `localhost` (CN=localhost, no SAN) while the client connects by IP, so the
    // connection URL host (`127.0.0.1`) does not match the certificate. Verification must instead
    // use the configured `server_name`.
    #[tokio::test]
    async fn server_name_is_used_for_hostname_verification() {
        use std::{net::SocketAddr, pin::Pin};

        // Connects to `addr` by IP, driving `into_ssl` with `url_host` exactly as
        // `hyper-openssl` does (it passes the connection URL host).
        async fn connect(
            server_name: Option<&str>,
            skip_server_name: bool,
            url_host: &str,
            addr: SocketAddr,
        ) -> std::result::Result<(), String> {
            let settings = TlsSettings::from_options(Some(&TlsConfig {
                ca_file: Some("tests/data/ca/intermediate_server/certs/ca-chain.cert.pem".into()),
                server_name: server_name.map(Into::into),
                ..Default::default()
            }))
            .unwrap();

            let tcp = tokio::net::TcpStream::connect(addr)
                .await
                .map_err(|e| e.to_string())?;
            let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
            settings
                .apply_context(&mut builder)
                .map_err(|e| e.to_string())?;
            let mut config = builder.build().configure().unwrap();
            settings
                .apply_connect_configuration(&mut config, skip_server_name)
                .map_err(|e| e.to_string())?;
            let ssl = config.into_ssl(url_host).map_err(|e| e.to_string())?;
            let mut stream = tokio_openssl::SslStream::new(ssl, tcp).unwrap();
            Pin::new(&mut stream)
                .connect()
                .await
                .map_err(|e| e.to_string())
        }

        let server_settings = MaybeTlsSettings::from_config(
            Some(&TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    crt_file: Some(TEST_PEM_CRT_PATH.into()),
                    key_file: Some(TEST_PEM_KEY_PATH.into()),
                    ..Default::default()
                },
            }),
            true,
        )
        .unwrap();
        let reloader = server_settings
            .reloadable_acceptor()
            .unwrap()
            .expect("tls enabled, so an acceptor should exist");
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let mut listener = server_settings
            .bind_reloadable(&addr, Some(reloader))
            .await
            .unwrap();
        let local_addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            while let Ok(mut stream) = listener.accept().await {
                stream.handshake().await.ok();
            }
        });

        // With `server_name` set to the certificate's name, verification uses it and succeeds even
        // though the connection host is the IP. This passes only because `server_name` is applied
        // to hostname verification (the bug this fixes).
        connect(Some("localhost"), false, "127.0.0.1", local_addr)
            .await
            .expect("handshake should succeed when server_name matches the certificate");

        // Control: without `server_name`, verification falls back to the connection host (the IP),
        // which the certificate does not cover, so it fails. This proves verification is active.
        let error = connect(None, false, "127.0.0.1", local_addr)
            .await
            .expect_err("handshake should fail when verifying against the IP");
        assert!(
            error.contains("certificate verify failed"),
            "expected a certificate verification failure, got: {error}"
        );

        // With `skip_server_name` (as when dialing a proxy), the `server_name` override is ignored
        // and verification falls back to the connection host (the IP), which fails. This is what
        // keeps an HTTPS proxy's own certificate verified against the proxy host.
        let error = connect(Some("localhost"), true, "127.0.0.1", local_addr)
            .await
            .expect_err("handshake should fail when the server_name override is skipped");
        assert!(
            error.contains("certificate verify failed"),
            "expected a certificate verification failure, got: {error}"
        );

        server.abort();
    }

    #[test]
    fn tls_version_range_must_not_be_inverted() {
        let options = TlsConfig {
            min_tls_version: Some(TlsVersion::Tls13),
            max_tls_version: Some(TlsVersion::Tls12),
            ..Default::default()
        };
        let error = TlsSettings::from_options(Some(&options))
            .expect_err("an inverted version range should be rejected");
        assert!(
            matches!(
                error,
                TlsError::InvalidTlsVersionRange {
                    min: TlsVersion::Tls13,
                    max: TlsVersion::Tls12
                }
            ),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn equal_tls_version_bounds_are_accepted() {
        let options = TlsConfig {
            min_tls_version: Some(TlsVersion::Tls12),
            max_tls_version: Some(TlsVersion::Tls12),
            ..Default::default()
        };
        TlsSettings::from_options(Some(&options))
            .expect("pinning to a single version should be allowed");
    }

    #[test]
    fn tls_versions_deserialize_from_their_config_names() {
        for (name, expected) in [
            ("TLSv1", TlsVersion::Tls10),
            ("TLSv1.1", TlsVersion::Tls11),
            ("TLSv1.2", TlsVersion::Tls12),
            ("TLSv1.3", TlsVersion::Tls13),
        ] {
            let parsed: TlsVersion = serde_json::from_str(&format!("\"{name}\"")).unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_string(), name);
        }
    }

    #[test]
    fn unset_tls_versions_leave_the_context_untouched() {
        let settings = TlsSettings::from_options(None).unwrap();
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        let before = builder.options();

        settings.apply_context(&mut builder).unwrap();

        assert_eq!(builder.min_proto_version(), None);
        assert_eq!(builder.max_proto_version(), None);
        assert_eq!(builder.options(), before);
    }

    #[test]
    fn configured_tls_versions_bound_the_context() {
        let settings = TlsSettings::from_options(Some(&TlsConfig {
            min_tls_version: Some(TlsVersion::Tls12),
            max_tls_version: Some(TlsVersion::Tls12),
            ..Default::default()
        }))
        .unwrap();
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();

        settings.apply_context(&mut builder).unwrap();

        assert_eq!(builder.min_proto_version(), Some(SslVersion::TLS1_2));
        assert_eq!(builder.max_proto_version(), Some(SslVersion::TLS1_2));
    }

    // `SSL_CTX_set_min_proto_version(0)` -- what `set_min_proto_version(None)` compiles to --
    // clears any bound already in force rather than leaving it alone. A host that sets
    // `MinProtocol = TLSv1.2` in `openssl.cnf` has that bound applied when the context is
    // created, so configuring only `max_tls_version` must not wipe it and silently re-enable
    // TLS v1.0/v1.1. The pre-set bound here stands in for that host policy.
    #[test]
    fn configuring_only_a_maximum_preserves_an_existing_minimum() {
        let settings = TlsSettings::from_options(Some(&TlsConfig {
            max_tls_version: Some(TlsVersion::Tls13),
            ..Default::default()
        }))
        .unwrap();

        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder
            .set_min_proto_version(Some(SslVersion::TLS1_2))
            .unwrap();

        settings.apply_context(&mut builder).unwrap();

        assert_eq!(
            builder.min_proto_version(),
            Some(SslVersion::TLS1_2),
            "an unconfigured minimum must not clear a bound already in force"
        );
        assert_eq!(builder.max_proto_version(), Some(SslVersion::TLS1_3));
    }

    // The symmetric case: configuring only a minimum must not clear a host-supplied maximum.
    #[test]
    fn configuring_only_a_minimum_preserves_an_existing_maximum() {
        let settings = TlsSettings::from_options(Some(&TlsConfig {
            min_tls_version: Some(TlsVersion::Tls12),
            ..Default::default()
        }))
        .unwrap();

        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder
            .set_max_proto_version(Some(SslVersion::TLS1_2))
            .unwrap();

        settings.apply_context(&mut builder).unwrap();

        assert_eq!(builder.min_proto_version(), Some(SslVersion::TLS1_2));
        assert_eq!(
            builder.max_proto_version(),
            Some(SslVersion::TLS1_2),
            "an unconfigured maximum must not clear a bound already in force"
        );
    }

    // Applying a window must never relax an `SSL_OP_NO_*` restriction Vector did not set itself.
    #[test]
    fn version_window_does_not_clear_restrictions_vector_did_not_set() {
        let settings = TlsSettings::from_options(Some(&TlsConfig {
            min_tls_version: Some(TlsVersion::Tls10),
            ..Default::default()
        }))
        .unwrap();

        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder.set_options(SslOptions::NO_TLSV1 | SslOptions::NO_TLSV1_1);

        settings.apply_context(&mut builder).unwrap();

        let options = builder.options();
        assert!(
            options.contains(SslOptions::NO_TLSV1),
            "TLS v1.0 was disabled outside Vector and must stay disabled"
        );
        assert!(
            options.contains(SslOptions::NO_TLSV1_1),
            "TLS v1.1 was disabled outside Vector and must stay disabled"
        );
    }

    // A window that excludes TLS v1.3 must leave the acceptor profile's `NO_TLSV1_3` in place.
    #[test]
    fn window_excluding_tls13_leaves_the_no_tls13_option_set() {
        let settings = TlsSettings::from_options_base(
            Some(&TlsConfig {
                crt_file: Some(TEST_PEM_CRT_PATH.into()),
                key_file: Some(TEST_PEM_KEY_PATH.into()),
                max_tls_version: Some(TlsVersion::Tls12),
                ..Default::default()
            }),
            true,
        )
        .unwrap();

        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        settings.apply_context_base(&mut acceptor, true).unwrap();

        assert!(acceptor.options().contains(SslOptions::NO_TLSV1_3));
        assert_eq!(acceptor.max_proto_version(), Some(SslVersion::TLS1_2));
    }

    // The acceptor is built from `SslAcceptor::mozilla_intermediate`, which sets
    // `SSL_OP_NO_TLSv1_3`. That option outranks the min/max protocol version in OpenSSL, so
    // without clearing it a configured window that includes TLS v1.3 would silently exclude it.
    #[test]
    fn acceptor_tls_version_window_clears_the_no_tls13_option() {
        let settings = TlsSettings::from_options_base(
            Some(&TlsConfig {
                crt_file: Some(TEST_PEM_CRT_PATH.into()),
                key_file: Some(TEST_PEM_KEY_PATH.into()),
                min_tls_version: Some(TlsVersion::Tls12),
                ..Default::default()
            }),
            true,
        )
        .unwrap();

        // Mirrors `TlsSettings::acceptor`, which starts from Mozilla's v4 intermediate profile.
        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        assert!(
            acceptor.options().contains(SslOptions::NO_TLSV1_3),
            "precondition: the profile this test guards against disables TLS v1.3"
        );

        settings.apply_context_base(&mut acceptor, true).unwrap();

        assert_eq!(acceptor.min_proto_version(), Some(SslVersion::TLS1_2));
        assert!(
            !acceptor.options().contains(SslOptions::NO_TLSV1_3),
            "TLS v1.3 must be available once a version window is configured"
        );
    }

    // End-to-end proof that the negotiated version is actually constrained on the wire: a server
    // pinned to TLS v1.2 must reject a client that only offers TLS v1.3, and accept one that
    // offers TLS v1.2.
    #[tokio::test]
    async fn min_tls_version_is_enforced_during_the_handshake() {
        use std::{net::SocketAddr, pin::Pin};

        async fn connect(
            client: &TlsConfig,
            addr: SocketAddr,
        ) -> std::result::Result<String, String> {
            let settings = TlsSettings::from_options(Some(client)).map_err(|e| e.to_string())?;
            let tcp = tokio::net::TcpStream::connect(addr)
                .await
                .map_err(|e| e.to_string())?;
            let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
            settings
                .apply_context(&mut builder)
                .map_err(|e| e.to_string())?;
            let ssl = builder
                .build()
                .configure()
                .unwrap()
                .into_ssl("localhost")
                .map_err(|e| e.to_string())?;
            let mut stream = tokio_openssl::SslStream::new(ssl, tcp).unwrap();
            Pin::new(&mut stream)
                .connect()
                .await
                .map_err(|e| e.to_string())?;
            Ok(stream.ssl().version_str().to_string())
        }

        let server_settings = MaybeTlsSettings::from_config(
            Some(&TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    crt_file: Some(TEST_PEM_CRT_PATH.into()),
                    key_file: Some(TEST_PEM_KEY_PATH.into()),
                    min_tls_version: Some(TlsVersion::Tls12),
                    max_tls_version: Some(TlsVersion::Tls12),
                    ..Default::default()
                },
            }),
            true,
        )
        .unwrap();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let mut listener = server_settings.bind(&addr).await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            while let Ok(mut stream) = listener.accept().await {
                stream.handshake().await.ok();
            }
        });

        let client = TlsConfig {
            ca_file: Some(TEST_PEM_INTERMEDIATE_CA_PATH.into()),
            ..Default::default()
        };

        // A client that can speak TLS v1.2 negotiates exactly that, and nothing newer.
        let version = connect(&client, local_addr)
            .await
            .expect("handshake should succeed at the server's pinned version");
        assert_eq!(version, "TLSv1.2");

        // A client that will only offer TLS v1.3 has no version in common with the server.
        let error = connect(
            &TlsConfig {
                min_tls_version: Some(TlsVersion::Tls13),
                ..client.clone()
            },
            local_addr,
        )
        .await
        .expect_err("handshake should fail when the version windows do not overlap");
        assert!(
            error.contains("protocol version") || error.contains("alert"),
            "expected a protocol version failure, got: {error}"
        );

        server.abort();
    }

    #[test]
    fn from_config_none() {
        assert!(MaybeTlsSettings::from_config(None, true).unwrap().is_raw());
        assert!(MaybeTlsSettings::from_config(None, false).unwrap().is_raw());
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
        let error = MaybeTlsSettings::from_config(Some(&config), true)
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
        MaybeTlsSettings::from_config(Some(&config), for_server)
            .expect("Failed to generate settings from config")
    }

    fn make_config(enabled: Option<bool>, set_crt: bool, set_key: bool) -> TlsEnableableConfig {
        TlsEnableableConfig {
            enabled,
            options: TlsConfig {
                crt_file: set_crt.then(|| TEST_PEM_CRT_PATH.into()),
                key_file: set_key.then(|| TEST_PEM_KEY_PATH.into()),
                ..Default::default()
            },
        }
    }
}
