use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    sync::{Arc, RwLock},
    time::Duration,
};

use async_compression::tokio::bufread;
use azure_core::{
    credentials::TokenCredential,
    http::{
        AsyncResponseBody, Context, HttpClient, Request, Transport, Url,
        policies::{Policy, PolicyResult},
    },
};
use azure_storage_blob::{BlobContainerClient, BlobContainerClientOptions};
use azure_storage_queue::{QueueClient, QueueClientOptions};
use futures::{StreamExt, TryStreamExt, stream};
use snafu::Snafu;
use tokio_util::io::StreamReader;
use vector_common::compression::gzip_multiple_decoder;
use vector_lib::{
    codecs::{
        NewlineDelimitedDecoderConfig,
        decoding::{
            DeserializerConfig, FramingConfig, NewlineDelimitedDecoderOptions, OversizedAction,
        },
    },
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::owned_value_path,
    sensitive_string::SensitiveString,
};
use vrl::value::{Kind, kind::Collection};

use super::util::MultilineConfig;
use crate::{
    codecs::DecodingConfig,
    config::{
        ProxyConfig, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
    },
    line_agg,
    serde::{bool_or_struct, default_decoding},
    sinks::azure_common::{
        config::{AzureAuthentication, AzureBlobTlsConfig},
        connection_string::{Auth, ParsedConnectionString},
        shared_key_policy::SharedKeyAuthorizationPolicy,
    },
};

#[cfg(all(test, feature = "azure-blob-integration-tests"))]
mod integration_tests;
pub mod queue;

/// The storage service version sent with Shared Key signed requests. Must be a version
/// supported by Azurite for the integration tests to pass.
const STORAGE_SERVICE_VERSION: &str = "2025-11-05";

/// Connection timeout for the custom transport, matching the Azure SDK default. Requires the
/// `azure_core/tokio` feature: without it the SDK spawns partitioned downloads (blobs over
/// 4 MiB) onto plain threads, where building the timer panics.
const AZURE_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Azurite's queue service port. The blob service uses 10000 and the table service 10002.
const DEV_STORAGE_QUEUE_PORT: u16 = 10001;

/// Compression scheme for blobs retrieved from Azure Blob Storage.
#[configurable_component]
#[configurable(metadata(docs::advanced))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    /// Automatically attempt to determine the compression scheme.
    ///
    /// The compression scheme of the blob is determined from its `Content-Encoding` and
    /// `Content-Type` metadata, as well as the blob name suffix (for example, `.gz`).
    ///
    /// It is set to `none` if the compression scheme cannot be determined.
    #[default]
    Auto,

    /// Uncompressed.
    None,

    /// GZIP.
    Gzip,

    /// ZSTD.
    Zstd,
}

/// Strategies for consuming blobs from Azure Blob Storage.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(rename_all = "snake_case")]
enum Strategy {
    /// Consumes blobs by processing `Microsoft.Storage.BlobCreated` notifications delivered by an
    /// Event Grid subscription to an [Azure Storage Queue][azure_queue].
    ///
    /// [azure_queue]: https://learn.microsoft.com/azure/storage/queues/storage-queues-introduction
    #[default]
    StorageQueue,
}

/// Configuration for the `azure_blob` source.
#[configurable_component(source("azure_blob", "Collect logs from Azure Blob Storage."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(default, deny_unknown_fields)]
pub struct AzureBlobConfig {
    /// The Azure Blob Storage Account connection string.
    ///
    /// Authentication with an access key or shared access signature (SAS) are supported
    /// authentication methods. The connection string is also used to derive the blob and
    /// queue service endpoints.
    #[configurable(metadata(
        docs::warnings = "Access keys and SAS tokens can be used to gain unauthorized access to Azure Storage \
        resources. Numerous security breaches have occurred due to leaked connection strings. It is important to keep \
        connection strings secure and not expose them in logs, error messages, or version control systems."
    ))]
    #[configurable(metadata(
        docs::examples = "DefaultEndpointsProtocol=https;AccountName=mylogstorage;AccountKey=storageaccountkeybase64encoded;EndpointSuffix=core.windows.net"
    ))]
    #[configurable(metadata(
        docs::examples = "BlobEndpoint=https://mylogstorage.blob.core.windows.net/;QueueEndpoint=https://mylogstorage.queue.core.windows.net/;SharedAccessSignature=generatedsastoken"
    ))]
    #[configurable(metadata(docs::examples = "AccountName=mylogstorage"))]
    connection_string: Option<SensitiveString>,

    /// The Azure Blob Storage Account name.
    ///
    /// If provided, this is used instead of the `connection_string` and requires `auth` to be
    /// configured. Both the blob and queue service endpoints are derived from the account name.
    #[configurable(metadata(docs::examples = "mylogstorage"))]
    account_name: Option<String>,

    /// The Azure Blob Storage service endpoint.
    ///
    /// Useful for Azurite, sovereign clouds, or private endpoints. Requires `auth` to be
    /// configured, and `queue_endpoint` to be provided as well when `account_name` is not set.
    #[configurable(metadata(docs::examples = "https://mylogstorage.blob.core.windows.net/"))]
    blob_endpoint: Option<String>,

    /// The Azure Queue Storage service endpoint.
    ///
    /// By default the queue endpoint is derived from `account_name` or the connection string.
    #[configurable(metadata(docs::examples = "https://mylogstorage.queue.core.windows.net/"))]
    queue_endpoint: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    auth: Option<AzureAuthentication>,

    /// Configuration options for the Storage Queue.
    queue: Option<queue::Config>,

    /// The compression scheme used for decompressing blobs retrieved from Azure Blob Storage.
    compression: Compression,

    /// The strategy to use to consume blobs from Azure Blob Storage.
    #[configurable(metadata(docs::hidden))]
    strategy: Strategy,

    /// Multiline aggregation configuration.
    ///
    /// If not specified, multiline aggregation is disabled.
    #[configurable(derived)]
    multiline: Option<MultilineConfig>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    #[configurable(derived)]
    #[serde(default = "default_framing")]
    #[derivative(Default(value = "default_framing()"))]
    pub framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,

    #[configurable(derived)]
    tls: Option<AzureBlobTlsConfig>,
}

const fn default_framing() -> FramingConfig {
    // This mirrors the `aws_s3` source's historical default.
    FramingConfig::NewlineDelimited(NewlineDelimitedDecoderConfig {
        newline_delimited: NewlineDelimitedDecoderOptions {
            max_length: None,
            oversized_action: OversizedAction::Drop,
        },
    })
}

impl_generate_config_from_default!(AzureBlobConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "azure_blob")]
impl SourceConfig for AzureBlobConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let multiline_config: Option<line_agg::Config> = self
            .multiline
            .as_ref()
            .map(|config| config.try_into())
            .transpose()?;

        match self.strategy {
            Strategy::StorageQueue => Ok(Box::pin(
                self.create_queue_ingestor(multiline_config, &cx.proxy, log_namespace)
                    .await?
                    .run(cx, self.acknowledgements, log_namespace),
            )),
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let mut schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("container"))),
                &owned_value_path!("container"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("blob"))),
                &owned_value_path!("blob"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("storage_account"))),
                &owned_value_path!("storage_account"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                None,
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some("timestamp"),
            )
            .with_standard_vector_source_metadata()
            // for metadata that is added to the events dynamically from the blob metadata
            .with_source_metadata(
                Self::NAME,
                None,
                &owned_value_path!("metadata"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            );

        // for metadata that is added to the events dynamically from the blob metadata
        if log_namespace == LogNamespace::Legacy {
            schema_definition = schema_definition.unknown_fields(Kind::bytes());
        }

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

impl AzureBlobConfig {
    async fn create_client_source(
        &self,
        proxy: &ProxyConfig,
    ) -> crate::Result<AzureStorageClientSource> {
        let connection_string: String = match (
            &self.connection_string,
            &self.account_name,
            &self.blob_endpoint,
        ) {
            (Some(connstr), None, None) => connstr.inner().into(),
            (None, Some(account_name), None) => {
                if self.auth.is_none() {
                    return Err(
                        "`auth` configuration must be provided when using `account_name`".into(),
                    );
                }
                format!("AccountName={account_name}")
            }
            (None, None, Some(blob_endpoint)) => {
                if self.auth.is_none() {
                    return Err(
                        "`auth` configuration must be provided when using `blob_endpoint`".into(),
                    );
                }
                if self.queue_endpoint.is_none() {
                    return Err("`queue_endpoint` must be provided when using `blob_endpoint` without `account_name`".into());
                }
                // BlobEndpoint must always end in a trailing slash
                let blob_endpoint = if blob_endpoint.ends_with('/') {
                    blob_endpoint.clone()
                } else {
                    format!("{blob_endpoint}/")
                };
                format!("BlobEndpoint={blob_endpoint}")
            }
            (None, None, None) => {
                return Err("One of `connection_string`, `account_name`, or `blob_endpoint` must be provided".into());
            }
            (Some(_), Some(_), _) => {
                return Err("Cannot provide both `connection_string` and `account_name`".into());
            }
            (Some(_), _, Some(_)) => {
                return Err("Cannot provide both `connection_string` and `blob_endpoint`".into());
            }
            (None, Some(account_name), Some(blob_endpoint)) => {
                if self.auth.is_none() {
                    return Err("`auth` configuration must be provided when using `account_name` and `blob_endpoint`".into());
                }
                let blob_endpoint = if blob_endpoint.ends_with('/') {
                    blob_endpoint.clone()
                } else {
                    format!("{blob_endpoint}/")
                };
                format!("AccountName={account_name};BlobEndpoint={blob_endpoint}")
            }
        };

        AzureStorageClientSource::new(
            connection_string,
            self.queue_endpoint.clone(),
            self.auth.clone(),
            proxy.clone(),
            self.tls.clone(),
        )
        .await
    }

    async fn create_queue_ingestor(
        &self,
        multiline: Option<line_agg::Config>,
        proxy: &ProxyConfig,
        log_namespace: LogNamespace,
    ) -> crate::Result<queue::Ingestor> {
        let clients = self.create_client_source(proxy).await?;

        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        match self.queue {
            Some(ref queue) => {
                let ingestor = queue::Ingestor::new(
                    clients,
                    queue.clone(),
                    self.compression,
                    multiline,
                    decoder,
                )?;

                Ok(ingestor)
            }
            None => Err(CreateQueueIngestorError::ConfigMissing {}.into()),
        }
    }
}

#[derive(Debug, Snafu)]
enum CreateQueueIngestorError {
    #[snafu(display("Configuration for `queue` required when strategy=storage_queue"))]
    ConfigMissing,
}

/// Shared factory for Azure Storage clients, mirroring the sink's `build_client`
/// (`src/sinks/azure_blob/config.rs`) so the same configuration works for both.
pub struct AzureStorageClientSource {
    raw_connection_string: String,
    parsed: ParsedConnectionString,
    queue_endpoint: Option<String>,
    credential: Option<Arc<dyn TokenCredential>>,
    shared_key: Option<(String, String)>,
    proxy: ProxyConfig,
    ca_pem: Option<Vec<u8>>,
    /// HTTP clients, cached per host. See `build_transport`.
    http_clients: RwLock<HashMap<String, Arc<dyn HttpClient>>>,
}

impl AzureStorageClientSource {
    async fn new(
        connection_string: String,
        queue_endpoint: Option<String>,
        auth: Option<AzureAuthentication>,
        proxy: ProxyConfig,
        tls: Option<AzureBlobTlsConfig>,
    ) -> crate::Result<Self> {
        let parsed = ParsedConnectionString::parse(&connection_string)
            .map_err(|e| format!("Invalid connection string: {e}"))?;

        let mut credential: Option<Arc<dyn TokenCredential>> = None;
        let mut shared_key: Option<(String, String)> = None;

        match (parsed.auth(), &auth) {
            (Auth::None, None) => {
                warn!("No authentication method provided, requests will be anonymous.");
            }
            (Auth::Sas { .. }, None) => {
                info!("Using SAS token authentication.");
            }
            (
                Auth::SharedKey {
                    account_name,
                    account_key,
                },
                None,
            ) => {
                info!("Using Shared Key authentication.");
                shared_key = Some((account_name, account_key));
            }
            (Auth::None, Some(auth_config)) => {
                info!("Using Azure Authentication method.");
                let credential_result = auth_config
                    .credential()
                    .await
                    .map_err(|e| format!("Failed to configure Azure Authentication: {e}"))?;
                credential = Some(credential_result);
            }
            (Auth::Sas { .. }, Some(_)) => {
                return Err(
                    "Cannot use both SAS token and another Azure Authentication method at the same time".into(),
                );
            }
            (Auth::SharedKey { .. }, Some(_)) => {
                return Err(
                    "Cannot use both Shared Key and another Azure Authentication method at the same time".into(),
                );
            }
        }

        let ca_pem = match &tls {
            Some(AzureBlobTlsConfig {
                ca_file: Some(ca_file),
            }) => {
                let mut buf = Vec::new();
                File::open(ca_file)
                    .map_err(|e| format!("Failed to open TLS CA file {}: {e}", ca_file.display()))?
                    .read_to_end(&mut buf)
                    .map_err(|e| {
                        format!("Failed to read TLS CA file {}: {e}", ca_file.display())
                    })?;
                reqwest_13::Certificate::from_pem(&buf)
                    .map_err(|e| format!("Invalid TLS CA file {}: {e}", ca_file.display()))?;
                info!("Adding TLS root certificate from {}.", ca_file.display());
                Some(buf)
            }
            _ => None,
        };

        Ok(Self {
            raw_connection_string: connection_string,
            parsed,
            queue_endpoint,
            credential,
            shared_key,
            proxy,
            ca_pem,
            http_clients: RwLock::new(HashMap::new()),
        })
    }

    pub fn account_name(&self) -> Option<String> {
        if let Some(account) = self.parsed.account_name.as_ref() {
            return Some(account.clone());
        }
        if let Some(blob_endpoint) = self.parsed.blob_endpoint.as_deref() {
            return queue::account_from_url(blob_endpoint);
        }
        None
    }

    /// Resolution order, mirroring `ParsedConnectionString::blob_account_endpoint`:
    /// 1. The explicit `queue_endpoint` configuration option.
    /// 2. A `QueueEndpoint` key in the connection string, which `ParsedConnectionString`
    ///    ignores as an unknown key.
    /// 3. Development storage: `{proto}://127.0.0.1:10001/{account}`.
    /// 4. Public cloud: `{proto}://{account}.queue.{endpoint_suffix}`.
    fn queue_account_endpoint(&self) -> crate::Result<String> {
        if let Some(explicit) = self.queue_endpoint.as_ref() {
            return Ok(explicit.clone());
        }

        if let Some(from_cs) = connection_string_value(&self.raw_connection_string, "QueueEndpoint")
        {
            return Ok(from_cs);
        }

        let proto = self.parsed.default_protocol();

        let account_name = self.parsed.account_name.as_ref().ok_or(
            "Could not determine Queue endpoint: `queue_endpoint` or an account name is required",
        )?;

        if self.parsed.use_development_storage {
            let base = match self.parsed.development_storage_proxy_uri.as_deref() {
                Some(proxy_uri) => dev_storage_queue_base(proxy_uri, &proto),
                None => format!("{proto}://127.0.0.1:{DEV_STORAGE_QUEUE_PORT}"),
            };
            return Ok(format!("{base}/{account_name}"));
        }

        let suffix = self.parsed.endpoint_suffix();
        Ok(format!("{proto}://{account_name}.queue.{suffix}"))
    }

    /// The endpoint may already carry a query string (such as a SAS embedded in an explicit
    /// `queue_endpoint`), so the queue name is inserted into the path rather than appended.
    fn queue_url(&self, queue_name: &str) -> crate::Result<String> {
        let base = self.queue_account_endpoint()?;
        let (base_path, base_query) = match base.split_once('?') {
            Some((path, query)) => (path, Some(query)),
            None => (base.as_str(), None),
        };
        let url = format!("{}/{queue_name}", base_path.trim_end_matches('/'));
        let url = append_query_segment(&url, base_query);
        Ok(append_query_segment(
            &url,
            self.parsed.shared_access_signature.as_deref(),
        ))
    }

    /// Clients are cached per host because the `no_proxy` bypass is host-specific but a fresh
    /// connection pool and TLS root store are expensive to rebuild. `container_client` is called
    /// lazily from the async ingestion path, so without the cache every first-sight container
    /// would build one on a runtime worker.
    fn build_transport(&self, url: &Url) -> crate::Result<Transport> {
        // Keyed on exactly what `build_http_client`'s proxy decision reads, so a cache hit can
        // never hand back a client built for a different bypass outcome.
        let key = format!(
            "{}://{}:{}",
            url.scheme(),
            url.host_str().unwrap_or_default(),
            url.port().map(|port| port.to_string()).unwrap_or_default(),
        );

        if let Some(client) = self.http_clients.read().expect("lock poisoned").get(&key) {
            return Ok(Transport::new(Arc::clone(client)));
        }

        let client: Arc<dyn HttpClient> = Arc::new(self.build_http_client(url)?);

        let mut clients = self.http_clients.write().expect("lock poisoned");
        let entry = clients.entry(key).or_insert(client);
        Ok(Transport::new(Arc::clone(entry)))
    }

    /// Construct a reqwest client, mirroring the sink's `build_client`: global proxy configuration
    /// (with per-host no-proxy bypass) plus an optional custom CA certificate.
    fn build_http_client(&self, url: &Url) -> crate::Result<reqwest_13::Client> {
        // Installing a transport skips the SDK's own `automatic_decompression: false` default.
        // Ask for the bytes exactly as stored: the source decompresses blob bodies itself from
        // `Content-Encoding`, and `BlobClient::download` documents that transparent
        // decompression can break partitioned downloads.
        let mut default_headers = reqwest_13::header::HeaderMap::new();
        default_headers.insert(
            reqwest_13::header::ACCEPT_ENCODING,
            reqwest_13::header::HeaderValue::from_static("identity"),
        );

        let mut reqwest_builder = reqwest_13::ClientBuilder::new()
            .connect_timeout(AZURE_CONNECT_TIMEOUT)
            .default_headers(default_headers)
            .redirect(reqwest_13::redirect::Policy::none());
        let bypass_proxy = {
            let host = url.host_str().unwrap_or("");
            let port = url.port();
            self.proxy.no_proxy.matches(host)
                || port
                    .map(|p| self.proxy.no_proxy.matches(&format!("{host}:{p}")))
                    .unwrap_or(false)
        };
        if bypass_proxy || !self.proxy.enabled {
            // Ensure no proxy (and disable any potential system proxy auto-detection)
            reqwest_builder = reqwest_builder.no_proxy();
        } else {
            if let Some(http) = &self.proxy.http {
                let p = reqwest_13::Proxy::http(http)
                    .map_err(|e| format!("Invalid HTTP proxy URL: {e}"))?;
                reqwest_builder = reqwest_builder.proxy(p);
            }
            if let Some(https) = &self.proxy.https {
                let p = reqwest_13::Proxy::https(https)
                    .map_err(|e| format!("Invalid HTTPS proxy URL: {e}"))?;
                reqwest_builder = reqwest_builder.proxy(p);
            }
        }

        if let Some(ca_pem) = &self.ca_pem {
            let cert = reqwest_13::Certificate::from_pem(ca_pem)
                .map_err(|e| format!("Invalid TLS root certificate: {e}"))?;
            reqwest_builder = reqwest_builder.add_root_certificate(cert);
        }

        reqwest_builder
            .build()
            .map_err(|e| format!("Failed to build reqwest client: {e}").into())
    }

    fn shared_key_policy(&self) -> crate::Result<Option<Arc<SharedKeyAuthorizationPolicy>>> {
        self.shared_key
            .as_ref()
            .map(|(account_name, account_key)| {
                SharedKeyAuthorizationPolicy::new(
                    account_name.clone(),
                    account_key.clone(),
                    String::from(STORAGE_SERVICE_VERSION),
                )
                .map(Arc::new)
                .map_err(|e| format!("Failed to create SharedKey policy: {e}").into())
            })
            .transpose()
    }

    pub(super) fn queue_client(&self, queue_name: &str) -> crate::Result<QueueClient> {
        let queue_url = self.queue_url(queue_name)?;
        let url = Url::parse(&queue_url).map_err(|e| format!("Invalid queue URL: {e}"))?;

        let mut options = QueueClientOptions::default();
        if let Some(policy) = self.shared_key_policy()? {
            options
                .client_options
                .per_call_policies
                .push(Arc::new(ContentLengthPolicy));
            options.client_options.per_call_policies.push(policy);
        }
        options.client_options.transport = Some(self.build_transport(&url)?);

        let client = QueueClient::new(url, self.credential.clone(), Some(options))
            .map_err(|e| format!("{e}"))?;
        Ok(client)
    }

    pub(super) fn container_client(
        &self,
        container_name: &str,
    ) -> crate::Result<BlobContainerClient> {
        let container_url = self
            .parsed
            .container_url(container_name)
            .map_err(|e| format!("Failed to build container URL: {e}"))?;
        let url = Url::parse(&container_url).map_err(|e| format!("Invalid container URL: {e}"))?;

        let mut options = BlobContainerClientOptions::default();
        if let Some(policy) = self.shared_key_policy()? {
            options.client_options.per_call_policies.push(policy);
        }
        options.client_options.transport = Some(self.build_transport(&url)?);

        let client = BlobContainerClient::new(url, self.credential.clone(), Some(options))
            .map_err(|e| format!("{e}"))?;
        Ok(client)
    }
}

/// Sets the `Content-Length` header from the request body before signing.
///
/// The generated queue client leaves `Content-Length` to the HTTP transport, which runs after
/// the pipeline policies, so `SharedKeyAuthorizationPolicy` would sign an empty value while the
/// server verifies against the transmitted one. Must be pushed in front of the Shared Key policy.
#[derive(Debug)]
struct ContentLengthPolicy;

#[async_trait::async_trait]
impl Policy for ContentLengthPolicy {
    async fn send(
        &self,
        ctx: &Context,
        request: &mut Request,
        next: &[Arc<dyn Policy>],
    ) -> PolicyResult {
        if let Some(len) = request.body().len()
            && len > 0
        {
            request.insert_header("content-length", len.to_string());
        }
        next[0].send(ctx, request, &next[1..]).await
    }
}

fn connection_string_value(connection_string: &str, key: &str) -> Option<String> {
    connection_string.split(';').find_map(|seg| {
        let (k, v) = seg.trim().split_once('=')?;
        k.trim()
            .eq_ignore_ascii_case(key)
            .then(|| v.trim().to_string())
    })
}

/// Rewrite a `DevelopmentStorageProxyUri` so that it addresses the queue service.
///
/// The proxy URI is the *blob* base, so any port it carries is the blob port. The queue is served
/// on a different one, so the port is replaced rather than reused.
fn dev_storage_queue_base(proxy_uri: &str, proto: &str) -> String {
    let trimmed = proxy_uri.trim_end_matches('/');
    let (scheme, authority) = match trimmed.split_once("://") {
        Some((scheme, rest)) => (scheme, rest),
        None => (proto, trimmed),
    };
    let authority = authority.split('/').next().unwrap_or(authority);
    // Strip a trailing `:port` while leaving IPv6 literals (`[::1]`) intact.
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => authority,
    };
    format!("{scheme}://{host}:{DEV_STORAGE_QUEUE_PORT}")
}

fn append_query_segment(base_url: &str, sas: Option<&str>) -> String {
    match sas {
        None | Some("") => base_url.to_string(),
        Some(q) => {
            let sep = if base_url.contains('?') { '&' } else { '?' };
            format!("{base_url}{sep}{q}")
        }
    }
}

/// Wrap the blob body in a decompressing reader; an empty body yields an empty reader.
async fn blob_decoder(
    compression: Compression,
    blob_name: &str,
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    body: AsyncResponseBody,
) -> Box<dyn tokio::io::AsyncRead + Send + Unpin> {
    let mut body = body.map_err(std::io::Error::other);
    let first = match body.next().await {
        Some(first) => first,
        _ => {
            return Box::new(tokio::io::empty());
        }
    };

    use Compression::*;
    let compression = match compression {
        // `first` is borrowed here and handed to the reader below untouched.
        Auto => match determine_compression(content_encoding, content_type, blob_name) {
            Some((inferred, source)) => verify_inferred_compression(
                inferred,
                source,
                first.as_ref().map(|bytes| bytes.as_ref()).unwrap_or(&[]),
                blob_name,
            ),
            Option::None => Compression::None,
        },
        explicit => explicit,
    };

    let r = tokio::io::BufReader::new(StreamReader::new(stream::iter(Some(first)).chain(body)));

    match compression {
        Auto => unreachable!(), // is mapped above
        None => Box::new(r),
        Gzip => Box::new(gzip_multiple_decoder(r)),
        Zstd => Box::new({
            let mut decoder = bufread::ZstdDecoder::new(r);
            decoder.multiple_members(true);
            decoder
        }),
    }
}

/// Which piece of metadata selected a compression scheme, reported when it disagrees with the
/// blob's contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompressionSource {
    ContentEncoding,
    ContentType,
    BlobName,
}

impl CompressionSource {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ContentEncoding => "Content-Encoding",
            Self::ContentType => "Content-Type",
            Self::BlobName => "blob name suffix",
        }
    }
}

// try to determine the compression given the:
// * content-encoding
// * content-type
// * blob name (for file extension)
//
// It will use this information in this order
fn determine_compression(
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    blob_name: &str,
) -> Option<(Compression, CompressionSource)> {
    content_encoding
        .and_then(content_encoding_to_compression)
        .map(|compression| (compression, CompressionSource::ContentEncoding))
        .or_else(|| {
            content_type
                .and_then(content_type_to_compression)
                .map(|compression| (compression, CompressionSource::ContentType))
        })
        .or_else(|| {
            blob_name_to_compression(blob_name)
                .map(|compression| (compression, CompressionSource::BlobName))
        })
}

/// The leading bytes that identify a compressed stream.
const GZIP_MAGIC: &[u8] = &[0x1f, 0x8b];
const ZSTD_MAGIC: &[u8] = &[0x28, 0xb5, 0x2f, 0xfd];

const fn compression_magic(compression: Compression) -> Option<&'static [u8]> {
    match compression {
        Compression::Gzip => Some(GZIP_MAGIC),
        Compression::Zstd => Some(ZSTD_MAGIC),
        Compression::Auto | Compression::None => None,
    }
}

/// Confirm an *inferred* compression scheme against the stream's leading bytes.
///
/// `compression: auto` picks a codec from metadata alone, so a mismatch downgrades to reading the
/// blob as-is rather than failing it. An explicitly configured codec is never second-guessed: a
/// blob that does not match is an error the operator asked to see.
fn verify_inferred_compression(
    inferred: Compression,
    source: CompressionSource,
    first: &[u8],
    blob_name: &str,
) -> Compression {
    let Some(magic) = compression_magic(inferred) else {
        return inferred;
    };

    // Too few bytes to judge, so trust the metadata rather than guess.
    if first.len() < magic.len() || first.starts_with(magic) {
        return inferred;
    }

    let leading = first
        .iter()
        .take(magic.len())
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ");

    warn!(
        message = "Blob metadata indicates a compression scheme that its contents do not match. Reading the blob undecompressed.",
        blob = %blob_name,
        detected = ?inferred,
        detected_from = %source.as_str(),
        leading_bytes = %leading,
    );

    Compression::None
}

fn content_encoding_to_compression(content_encoding: &str) -> Option<Compression> {
    match content_encoding {
        "gzip" => Some(Compression::Gzip),
        "zstd" => Some(Compression::Zstd),
        _ => None,
    }
}

fn content_type_to_compression(content_type: &str) -> Option<Compression> {
    match content_type {
        "application/gzip" | "application/x-gzip" => Some(Compression::Gzip),
        "application/zstd" => Some(Compression::Zstd),
        _ => None,
    }
}

fn blob_name_to_compression(blob_name: &str) -> Option<Compression> {
    let extension = std::path::Path::new(blob_name)
        .extension()
        .and_then(std::ffi::OsStr::to_str);

    use Compression::*;
    extension.and_then(|extension| match extension {
        "gz" => Some(Gzip),
        "zst" => Some(Zstd),
        _ => Option::None,
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn determine_compression() {
        use super::Compression;

        let cases = vec![
            ("out.log", Some("gzip"), None, Some(Compression::Gzip)),
            (
                "out.log",
                None,
                Some("application/gzip"),
                Some(Compression::Gzip),
            ),
            ("out.log.gz", None, None, Some(Compression::Gzip)),
            ("out.log.zst", None, None, Some(Compression::Zstd)),
            ("out.txt", None, None, None),
        ];
        for case in cases {
            let (blob_name, content_encoding, content_type, expected) = case;
            assert_eq!(
                super::determine_compression(content_encoding, content_type, blob_name)
                    .map(|(compression, _source)| compression),
                expected,
                "blob_name={blob_name:?} content_encoding={content_encoding:?} content_type={content_type:?}",
            );
        }
    }

    #[test]
    fn determine_compression_reports_its_source() {
        use super::CompressionSource;

        for (blob_name, content_encoding, content_type, expected) in [
            (
                "out.log",
                Some("gzip"),
                None,
                CompressionSource::ContentEncoding,
            ),
            (
                "out.log",
                None,
                Some("application/gzip"),
                CompressionSource::ContentType,
            ),
            ("out.log.gz", None, None, CompressionSource::BlobName),
        ] {
            let (_, source) =
                super::determine_compression(content_encoding, content_type, blob_name)
                    .expect("compression detected");
            assert_eq!(source, expected, "blob_name={blob_name:?}");
        }
    }

    #[test]
    fn inferred_compression_is_verified_against_the_leading_bytes() {
        use super::{CompressionSource, verify_inferred_compression};

        let verify = |first: &[u8], inferred| {
            verify_inferred_compression(inferred, CompressionSource::BlobName, first, "out.log.gz")
        };

        assert_eq!(
            verify(&[0x1f, 0x8b, 0x08, 0x00], Compression::Gzip),
            Compression::Gzip
        );
        assert_eq!(
            verify(&[0x28, 0xb5, 0x2f, 0xfd, 0x00], Compression::Zstd),
            Compression::Zstd
        );

        // Plain text under a `.gz` name: read it as-is rather than failing the whole blob.
        assert_eq!(
            verify(b"{\"message\":", Compression::Gzip),
            Compression::None
        );

        // A zstd frame is not gzip.
        assert_eq!(
            verify(&[0x28, 0xb5, 0x2f, 0xfd], Compression::Gzip),
            Compression::None
        );

        // Too few bytes to judge: keep trusting the metadata.
        assert_eq!(verify(&[0x1f], Compression::Gzip), Compression::Gzip);
        assert_eq!(verify(&[], Compression::Gzip), Compression::Gzip);

        assert_eq!(verify(b"plain", Compression::None), Compression::None);
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AzureBlobConfig>();
    }

    #[test]
    fn connection_string_value_extraction() {
        assert_eq!(
            connection_string_value(
                "AccountName=foo;QueueEndpoint=http://127.0.0.1:10001/foo",
                "QueueEndpoint"
            ),
            Some("http://127.0.0.1:10001/foo".to_string())
        );
        assert_eq!(
            connection_string_value("AccountName=foo;queueendpoint=http://q/", "QueueEndpoint"),
            Some("http://q/".to_string())
        );
        assert_eq!(
            connection_string_value("AccountName=foo", "QueueEndpoint"),
            None
        );
    }

    async fn clients_for(
        connection_string: &str,
        queue_endpoint: Option<&str>,
    ) -> AzureStorageClientSource {
        AzureStorageClientSource::new(
            connection_string.to_string(),
            queue_endpoint.map(ToOwned::to_owned),
            None,
            ProxyConfig::default(),
            None,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn queue_endpoint_resolution_public_cloud() {
        let clients = clients_for(
            "DefaultEndpointsProtocol=https;AccountName=myacct;AccountKey=base64==;EndpointSuffix=core.windows.net",
            None,
        )
        .await;
        assert_eq!(
            clients.queue_url("my-queue").unwrap(),
            "https://myacct.queue.core.windows.net/my-queue"
        );
    }

    #[tokio::test]
    async fn queue_endpoint_resolution_development_storage() {
        let clients = clients_for(
            "UseDevelopmentStorage=true;DefaultEndpointsProtocol=http;AccountName=devstoreaccount1",
            None,
        )
        .await;
        assert_eq!(
            clients.queue_url("my-queue").unwrap(),
            "http://127.0.0.1:10001/devstoreaccount1/my-queue"
        );
    }

    #[tokio::test]
    async fn queue_endpoint_resolution_development_storage_proxy_uri() {
        // `ParsedConnectionString::blob_account_endpoint` would return
        // `http://azurite:10000/devstoreaccount1` for the same connection string.
        for (proxy_uri, expected) in [
            ("http://azurite:10000", "http://azurite:10001"),
            ("http://azurite:10000/", "http://azurite:10001"),
            ("http://azurite", "http://azurite:10001"),
            ("azurite", "http://azurite:10001"),
            ("azurite:10000", "http://azurite:10001"),
        ] {
            let clients = clients_for(
                &format!(
                    "UseDevelopmentStorage=true;DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;DevelopmentStorageProxyUri={proxy_uri}"
                ),
                None,
            )
            .await;
            assert_eq!(
                clients.queue_url("my-queue").unwrap(),
                format!("{expected}/devstoreaccount1/my-queue"),
                "proxy_uri={proxy_uri:?}",
            );
        }
    }

    #[test]
    fn dev_storage_queue_base_leaves_ipv6_literals_intact() {
        assert_eq!(
            dev_storage_queue_base("http://[::1]:10000", "http"),
            "http://[::1]:10001"
        );
        assert_eq!(
            dev_storage_queue_base("http://[::1]", "http"),
            "http://[::1]:10001"
        );
    }

    #[tokio::test]
    async fn queue_endpoint_resolution_explicit_key() {
        let clients = clients_for(
            "AccountName=myacct;QueueEndpoint=http://localhost:14431/myacct/",
            None,
        )
        .await;
        assert_eq!(
            clients.queue_url("my-queue").unwrap(),
            "http://localhost:14431/myacct/my-queue"
        );
    }

    #[tokio::test]
    async fn queue_endpoint_resolution_explicit_option_takes_precedence() {
        let clients = clients_for(
            "AccountName=myacct;QueueEndpoint=http://ignored:1/myacct",
            Some("http://localhost:14431/myacct"),
        )
        .await;
        assert_eq!(
            clients.queue_url("my-queue").unwrap(),
            "http://localhost:14431/myacct/my-queue"
        );
    }

    #[tokio::test]
    async fn queue_url_appends_sas() {
        let clients = clients_for(
            "BlobEndpoint=https://myacct.blob.core.windows.net/;QueueEndpoint=https://myacct.queue.core.windows.net/;SharedAccessSignature=sv=2022-11-02&ss=bq",
            None,
        )
        .await;
        assert_eq!(
            clients.queue_url("my-queue").unwrap(),
            "https://myacct.queue.core.windows.net/my-queue?sv=2022-11-02&ss=bq"
        );
    }

    #[tokio::test]
    async fn queue_endpoint_with_query_string() {
        let clients = clients_for(
            "AccountName=myacct",
            Some("http://localhost:14431/myacct?sv=2022-11-02&sig=abc"),
        )
        .await;
        assert_eq!(
            clients.queue_url("my-queue").unwrap(),
            "http://localhost:14431/myacct/my-queue?sv=2022-11-02&sig=abc"
        );
    }

    #[test]
    fn append_query_segment_cases() {
        assert_eq!(append_query_segment("http://h/p", None), "http://h/p");
        assert_eq!(append_query_segment("http://h/p", Some("")), "http://h/p");
        assert_eq!(
            append_query_segment("http://h/p", Some("a=b")),
            "http://h/p?a=b"
        );
        assert_eq!(
            append_query_segment("http://h/p?x=1", Some("a=b")),
            "http://h/p?x=1&a=b"
        );
    }

    #[tokio::test]
    async fn config_validation_blob_endpoint_with_queue_endpoint() {
        let config = AzureBlobConfig {
            blob_endpoint: Some("http://localhost:10000/myacct".to_string()),
            queue_endpoint: Some("http://localhost:10001/myacct".to_string()),
            auth: Some(AzureAuthentication::MockCredential),
            ..Default::default()
        };
        let clients = config
            .create_client_source(&ProxyConfig::default())
            .await
            .unwrap();
        assert_eq!(
            clients.queue_url("my-queue").unwrap(),
            "http://localhost:10001/myacct/my-queue"
        );
    }

    #[tokio::test]
    async fn config_validation_blob_endpoint_requires_queue_endpoint() {
        let config = AzureBlobConfig {
            blob_endpoint: Some("http://localhost:10000/myacct".to_string()),
            auth: Some(AzureAuthentication::MockCredential),
            ..Default::default()
        };
        let err = config
            .create_client_source(&ProxyConfig::default())
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(err.to_string().contains("`queue_endpoint`"));
    }

    #[tokio::test]
    async fn config_validation_requires_queue() {
        let config = AzureBlobConfig {
            connection_string: Some("AccountName=foo;AccountKey=base64==".to_string().into()),
            ..Default::default()
        };
        let err = config
            .create_queue_ingestor(None, &ProxyConfig::default(), LogNamespace::Legacy)
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(err.to_string().contains("`queue` required"));
    }

    #[tokio::test]
    async fn config_validation_rejects_zero_poll_secs() {
        let config = AzureBlobConfig {
            connection_string: Some("AccountName=foo;AccountKey=base64==".to_string().into()),
            queue: Some(queue::Config {
                queue_name: "q".to_string(),
                poll_secs: 0,
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = config
            .create_queue_ingestor(None, &ProxyConfig::default(), LogNamespace::Legacy)
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(err.to_string().contains("poll_secs"), "{err}");
    }

    #[tokio::test]
    async fn config_validation_rejects_conflicting_auth() {
        let config = AzureBlobConfig {
            connection_string: Some("AccountName=foo;AccountKey=base64==".to_string().into()),
            auth: Some(AzureAuthentication::MockCredential),
            queue: Some(queue::Config {
                queue_name: "q".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = config
            .create_queue_ingestor(None, &ProxyConfig::default(), LogNamespace::Legacy)
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(err.to_string().contains("Shared Key"));
    }

    #[tokio::test]
    async fn config_validation_requires_auth_with_account_name() {
        let config = AzureBlobConfig {
            account_name: Some("foo".to_string()),
            queue: Some(queue::Config {
                queue_name: "q".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = config
            .create_queue_ingestor(None, &ProxyConfig::default(), LogNamespace::Legacy)
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(err.to_string().contains("`auth`"));
    }
}
