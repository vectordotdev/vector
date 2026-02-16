//! Azure Event Hubs source.
//! Collects events from Azure Event Hubs using the `azure_messaging_eventhubs` crate.
//!
//! Supports two authentication modes:
//! - **Connection string**: Provide `connection_string` with SAS credentials.
//! - **Azure Identity**: Provide `namespace` and `event_hub_name`; authentication
//!   is handled by `azure_identity::ManagedIdentityCredential` (or other
//!   `TokenCredential` implementations via environment configuration).

use std::sync::Arc;

use azure_core::credentials::{AccessToken, TokenCredential, TokenRequestOptions};
use azure_messaging_eventhubs::{
    ConsumerClient, OpenReceiverOptions, StartLocation, StartPosition,
};
use futures_util::StreamExt;
use openssl::{hash::MessageDigest, pkey::PKey, sign::Signer};
use tokio::time::{Duration, sleep};
use tokio_util::codec::FramedRead;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{
        Decoder, DecodingConfig, StreamDecodingError,
        decoding::{DeserializerConfig, FramingConfig},
    },
    config::{LegacyKey, LogNamespace, SourceAcknowledgementsConfig, SourceOutput},
    configurable::configurable_component,
    event::Event,
    sensitive_string::SensitiveString,
    shutdown::ShutdownSignal,
};
use vrl::{owned_value_path, path, value::Kind};

use crate::{
    SourceSender,
    config::{SourceConfig, SourceContext},
    internal_events::StreamClosedError,
    serde::{default_decoding, default_framing_message_based},
};

/// Configuration for the `azure_event_hubs` source.
#[configurable_component(source("azure_event_hubs", "Collect events from Azure Event Hubs."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AzureEventHubsSourceConfig {
    /// The connection string for the Event Hubs namespace.
    ///
    /// Must include `Endpoint`, `SharedAccessKeyName`, and `SharedAccessKey`.
    /// Optionally includes `EntityPath` for the Event Hub name.
    ///
    /// If not set, authentication falls back to `azure_identity` (e.g., Managed Identity).
    /// In that case, `namespace` and `event_hub_name` must be provided.
    #[configurable(metadata(
        docs::examples = "Endpoint=sb://mynamespace.servicebus.windows.net/;SharedAccessKeyName=mykeyname;SharedAccessKey=mykey;EntityPath=my-event-hub"
    ))]
    pub connection_string: Option<SensitiveString>,

    /// The fully qualified Event Hubs namespace host.
    ///
    /// Required when not using a connection string.
    /// For example: `mynamespace.servicebus.windows.net`.
    #[configurable(metadata(docs::examples = "mynamespace.servicebus.windows.net"))]
    pub namespace: Option<String>,

    /// The name of the Event Hub to consume from.
    ///
    /// Required if the connection string does not include `EntityPath`.
    #[configurable(metadata(docs::examples = "my-event-hub"))]
    pub event_hub_name: Option<String>,

    /// The consumer group to use.
    #[serde(default = "default_consumer_group")]
    #[configurable(metadata(docs::examples = "$Default"))]
    pub consumer_group: String,

    /// The partition IDs to consume from.
    ///
    /// If empty or not specified, all partitions are consumed automatically.
    /// Provide specific IDs (e.g., `["0", "1"]`) to consume a subset.
    #[serde(default)]
    pub partition_ids: Vec<String>,

    /// Where to start reading events from.
    ///
    /// Possible values: `latest`, `earliest`.
    #[serde(default = "default_start_position")]
    #[configurable(metadata(docs::examples = "latest"))]
    pub start_position: String,

    /// Framing configuration.
    #[serde(default = "default_framing_message_based")]
    #[configurable(derived)]
    pub framing: FramingConfig,

    /// Decoding configuration.
    #[serde(default = "default_decoding")]
    #[configurable(derived)]
    pub decoding: DeserializerConfig,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "crate::serde::bool_or_struct")]
    pub acknowledgements: SourceAcknowledgementsConfig,

    /// The log namespace to use for this source.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

impl Default for AzureEventHubsSourceConfig {
    fn default() -> Self {
        Self {
            connection_string: None,
            namespace: None,
            event_hub_name: None,
            consumer_group: default_consumer_group(),
            partition_ids: Vec::new(),
            start_position: default_start_position(),
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            acknowledgements: Default::default(),
            log_namespace: None,
        }
    }
}

fn default_consumer_group() -> String {
    "$Default".to_string()
}

fn default_start_position() -> String {
    "latest".to_string()
}

impl_generate_config_from_default!(AzureEventHubsSourceConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "azure_event_hubs")]
impl SourceConfig for AzureEventHubsSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let (namespace, event_hub_name, credential, custom_endpoint) = build_credential(
            self.connection_string.as_ref(),
            self.namespace.as_deref(),
            self.event_hub_name.as_deref(),
        )?;

        let event_hub_name_for_metrics = event_hub_name.clone();

        let mut builder =
            ConsumerClient::builder().with_consumer_group(self.consumer_group.clone());
        if let Some(endpoint) = custom_endpoint {
            builder = builder.with_custom_endpoint(endpoint);
        }
        let client = builder
            .open(&namespace, event_hub_name, credential)
            .await
            .map_err(|e| format!("Failed to open Event Hubs consumer: {e}"))?;

        let partition_ids = if self.partition_ids.is_empty() {
            // Auto-discover all partitions
            let props = client
                .get_eventhub_properties()
                .await
                .map_err(|e| format!("Failed to get Event Hub properties: {e}"))?;
            info!(
                message = "Auto-discovered partitions.",
                partitions = ?props.partition_ids,
            );
            props.partition_ids
        } else {
            self.partition_ids.clone()
        };

        let start_position = self.start_position.clone();

        Ok(Box::pin(azure_event_hubs_source(
            client,
            partition_ids,
            event_hub_name_for_metrics,
            start_position,
            decoder,
            cx.shutdown,
            cx.out,
            log_namespace,
        )))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("partition_id"))),
                &owned_value_path!("partition_id"),
                Kind::bytes(),
                Some("partition_id"),
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!(
                    "sequence_number"
                ))),
                &owned_value_path!("sequence_number"),
                Kind::integer(),
                Some("sequence_number"),
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("offset"))),
                &owned_value_path!("offset"),
                Kind::bytes(),
                Some("offset"),
            );
        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

async fn azure_event_hubs_source(
    client: ConsumerClient,
    partition_ids: Vec<String>,
    event_hub_name: String,
    start_position: String,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: SourceSender,
    log_namespace: LogNamespace,
) -> Result<(), ()> {
    let client = Arc::new(client);
    let shutdown = shutdown;

    let mut tasks = Vec::new();
    for partition_id in partition_ids {
        let client = Arc::clone(&client);
        let decoder = decoder.clone();
        let start_position = start_position.clone();
        let event_hub_name = event_hub_name.clone();
        let mut out = out.clone();
        let mut shutdown = shutdown.clone();

        tasks.push(tokio::spawn(async move {
            partition_receiver(
                client,
                partition_id,
                event_hub_name,
                start_position,
                decoder,
                &mut shutdown,
                &mut out,
                log_namespace,
            )
            .await
        }));
    }

    // Wait for shutdown or any task to complete
    futures_util::future::select_all(tasks)
        .await
        .0
        .unwrap_or(Ok(()))
}

const RECONNECT_BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const RECONNECT_BACKOFF_MAX: Duration = Duration::from_secs(30);

async fn partition_receiver(
    client: Arc<ConsumerClient>,
    partition_id: String,
    event_hub_name: String,
    start_position: String,
    decoder: Decoder,
    shutdown: &mut ShutdownSignal,
    out: &mut SourceSender,
    log_namespace: LogNamespace,
) -> Result<(), ()> {
    use crate::internal_events::azure_event_hubs::source::{
        AzureEventHubsBytesReceived, AzureEventHubsEventsReceived,
    };

    let start_loc = match start_position.as_str() {
        "earliest" => StartLocation::Earliest,
        _ => StartLocation::Latest,
    };

    let mut backoff = RECONNECT_BACKOFF_INITIAL;

    loop {
        let options = OpenReceiverOptions {
            start_position: Some(StartPosition {
                location: start_loc.clone(),
                inclusive: false,
            }),
            ..Default::default()
        };

        let receiver = match client
            .open_receiver_on_partition(partition_id.clone(), Some(options))
            .await
        {
            Ok(r) => {
                backoff = RECONNECT_BACKOFF_INITIAL;
                r
            }
            Err(e) => {
                emit!(
                    crate::internal_events::azure_event_hubs::source::AzureEventHubsConnectError {
                        error: e.to_string(),
                    }
                );
                tokio::select! {
                    _ = &mut *shutdown => return Ok(()),
                    _ = sleep(backoff) => {}
                }
                backoff = (backoff * 2).min(RECONNECT_BACKOFF_MAX);
                continue;
            }
        };

        let mut stream = receiver.stream_events();

        loop {
            tokio::select! {
                _ = &mut *shutdown => return Ok(()),
                msg = stream.next() => {
                    match msg {
                        Some(Ok(received)) => {
                            let body = received.event_data().body();
                            let data = match body {
                                Some(d) => d.to_vec(),
                                None => continue,
                            };

                            emit!(AzureEventHubsBytesReceived {
                                byte_size: data.len(),
                                protocol: "amqp",
                                event_hub_name: &event_hub_name,
                                partition_id: &partition_id,
                            });

                            let sequence_number = received.sequence_number();
                            let offset = received.offset().clone();

                            let mut framed = FramedRead::new(data.as_slice(), decoder.clone());
                            while let Some(next) = framed.next().await {
                                match next {
                                    Ok((events, _byte_size)) => {
                                        emit!(AzureEventHubsEventsReceived {
                                            count: events.len(),
                                            byte_size: events.estimated_json_encoded_size_of(),
                                            event_hub_name: &event_hub_name,
                                            partition_id: &partition_id,
                                        });

                                        let now = chrono::Utc::now();
                                        let events: Vec<Event> = events.into_iter().map(|mut event| {
                                            if let Event::Log(ref mut log) = event {
                                                log_namespace.insert_standard_vector_source_metadata(
                                                    log,
                                                    AzureEventHubsSourceConfig::NAME,
                                                    now,
                                                );

                                                log_namespace.insert_source_metadata(
                                                    AzureEventHubsSourceConfig::NAME,
                                                    log,
                                                    Some(LegacyKey::InsertIfEmpty(path!("partition_id"))),
                                                    path!("partition_id"),
                                                    partition_id.clone(),
                                                );

                                                if let Some(seq) = sequence_number {
                                                    log_namespace.insert_source_metadata(
                                                        AzureEventHubsSourceConfig::NAME,
                                                        log,
                                                        Some(LegacyKey::InsertIfEmpty(path!("sequence_number"))),
                                                        path!("sequence_number"),
                                                        seq,
                                                    );
                                                }

                                                if let Some(ref off) = offset {
                                                    log_namespace.insert_source_metadata(
                                                        AzureEventHubsSourceConfig::NAME,
                                                        log,
                                                        Some(LegacyKey::InsertIfEmpty(path!("offset"))),
                                                        path!("offset"),
                                                        off.clone(),
                                                    );
                                                }
                                            }
                                            event
                                        }).collect();

                                        if out.send_batch(events).await.is_err() {
                                            emit!(StreamClosedError { count: 1 });
                                            return Err(());
                                        }
                                    }
                                    Err(error) => {
                                        if !error.can_continue() {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        Some(Err(error)) => {
                            emit!(crate::internal_events::azure_event_hubs::source::AzureEventHubsReceiveError {
                                error: error.to_string(),
                            });
                            // Break inner loop to reconnect
                            break;
                        }
                        None => {
                            // Stream ended, try reconnecting
                            info!(
                                message = "Event Hubs stream ended, reconnecting.",
                                partition_id = %partition_id,
                            );
                            break;
                        }
                    }
                }
            }
        }

        // Backoff before reconnecting
        tokio::select! {
            _ = &mut *shutdown => return Ok(()),
            _ = sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(RECONNECT_BACKOFF_MAX);
    }
}

// --- Auth helpers ---

/// Builds a credential and resolves namespace + event hub name from config.
///
/// Returns `(namespace, event_hub_name, credential)`.
/// Result contains: (namespace, event_hub_name, credential, custom_endpoint_for_emulator)
pub(crate) fn build_credential(
    connection_string: Option<&SensitiveString>,
    namespace: Option<&str>,
    event_hub_name: Option<&str>,
) -> crate::Result<(String, String, Arc<dyn TokenCredential>, Option<String>)> {
    if let Some(cs) = connection_string {
        // Connection string auth: parse and create SAS credential
        let parsed = ParsedEventHubsConnectionString::parse(cs.inner())?;
        let eh_name = event_hub_name
            .map(|s| s.to_string())
            .or(parsed.entity_path.clone())
            .ok_or("Event Hub name must be specified via `event_hub_name` or `EntityPath` in the connection string")?;

        let ns = parsed
            .endpoint
            .trim_start_matches("sb://")
            .trim_end_matches('/')
            .to_string();

        let (credential, custom_endpoint): (Arc<dyn TokenCredential>, Option<String>) =
            if parsed.use_development_emulator {
                // Emulator mode: use dummy credential and plain AMQP endpoint
                let endpoint = format!("amqp://{}:5672", ns);
                (Arc::new(EmulatorCredential), Some(endpoint))
            } else {
                (
                    Arc::new(EventHubsSasCredential::new(
                        &parsed.endpoint,
                        &parsed.shared_access_key_name,
                        &parsed.shared_access_key,
                    )?),
                    None,
                )
            };

        Ok((ns, eh_name, credential, custom_endpoint))
    } else {
        // Azure Identity auth: use ManagedIdentityCredential
        let ns = namespace
            .ok_or("`namespace` is required when not using a connection string")?
            .to_string();
        let eh_name = event_hub_name
            .ok_or("`event_hub_name` is required when not using a connection string")?
            .to_string();

        let credential: Arc<dyn TokenCredential> =
            azure_identity::ManagedIdentityCredential::new(None)
                .map_err(|e| format!("Failed to create ManagedIdentityCredential: {e}"))?;

        Ok((ns, eh_name, credential, None))
    }
}

// --- Connection string parsing ---

#[derive(Debug)]
pub(crate) struct ParsedEventHubsConnectionString {
    pub endpoint: String,
    pub shared_access_key_name: String,
    pub shared_access_key: String,
    pub entity_path: Option<String>,
    pub use_development_emulator: bool,
}

impl ParsedEventHubsConnectionString {
    pub fn parse(connection_string: &str) -> crate::Result<Self> {
        let mut endpoint = None;
        let mut key_name = None;
        let mut key = None;
        let mut entity_path = None;
        let mut use_dev_emulator = false;

        for part in connection_string.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            // SharedAccessKey may contain '=' (base64 padding), use strip_prefix
            if let Some(v) = part.strip_prefix("SharedAccessKey=") {
                key = Some(v.to_string());
            } else if let Some((k, v)) = part.split_once('=') {
                match k.trim() {
                    "Endpoint" => endpoint = Some(v.trim().to_string()),
                    "SharedAccessKeyName" => key_name = Some(v.trim().to_string()),
                    "EntityPath" => entity_path = Some(v.trim().to_string()),
                    "UseDevelopmentEmulator" => {
                        use_dev_emulator = v.trim().eq_ignore_ascii_case("true");
                    }
                    _ => {}
                }
            }
        }

        Ok(Self {
            endpoint: endpoint.ok_or("Missing 'Endpoint' in connection string")?,
            shared_access_key_name: key_name
                .ok_or("Missing 'SharedAccessKeyName' in connection string")?,
            shared_access_key: key.ok_or("Missing 'SharedAccessKey' in connection string")?,
            entity_path,
            use_development_emulator: use_dev_emulator,
        })
    }
}

// --- SAS TokenCredential for connection string auth ---

/// A `TokenCredential` that generates Event Hubs SAS tokens from a shared access key.
// --- Emulator credential (dummy, no validation) ---

/// A dummy `TokenCredential` for the Event Hubs emulator.
///
/// The emulator does not validate SAS tokens, so this returns a fixed token.
#[derive(Debug)]
pub(crate) struct EmulatorCredential;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl TokenCredential for EmulatorCredential {
    async fn get_token(
        &self,
        _scopes: &[&str],
        _options: Option<TokenRequestOptions<'_>>,
    ) -> azure_core::Result<AccessToken> {
        let expires_on =
            azure_core::time::OffsetDateTime::now_utc() + std::time::Duration::from_secs(3600);
        Ok(AccessToken::new("emulator-dummy-token", expires_on))
    }
}

// --- SAS TokenCredential for connection string auth ---

///
/// Used when authenticating via connection string. The generated SAS token is compatible
/// with Event Hubs AMQP CBS (Claim-Based Security) authentication.
#[derive(Debug)]
pub(crate) struct EventHubsSasCredential {
    resource_uri: String,
    key_name: String,
    key: Vec<u8>,
}

impl EventHubsSasCredential {
    pub fn new(endpoint: &str, key_name: &str, key_b64: &str) -> crate::Result<Self> {
        let resource_uri = endpoint
            .trim_start_matches("sb://")
            .trim_end_matches('/')
            .to_string();

        let key = azure_core::base64::decode(key_b64.as_bytes())
            .map_err(|e| format!("Invalid SharedAccessKey base64: {e}"))?;

        Ok(Self {
            resource_uri,
            key_name: key_name.to_string(),
            key,
        })
    }

    fn generate_sas_token(&self, expiry_secs: u64) -> crate::Result<String> {
        let encoded_uri =
            url::form_urlencoded::byte_serialize(self.resource_uri.as_bytes()).collect::<String>();
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("Time error: {e}"))?
            .as_secs()
            + expiry_secs;

        let string_to_sign = format!("{}\n{}", encoded_uri, expiry);

        let pkey = PKey::hmac(&self.key).map_err(|e| format!("Failed to create HMAC key: {e}"))?;
        let mut signer = Signer::new(MessageDigest::sha256(), &pkey)
            .map_err(|e| format!("Failed to create signer: {e}"))?;
        signer
            .update(string_to_sign.as_bytes())
            .map_err(|e| format!("Signer update failed: {e}"))?;
        let signature = signer
            .sign_to_vec()
            .map_err(|e| format!("Signer sign failed: {e}"))?;
        let sig_b64 = azure_core::base64::encode(&signature);
        let sig_encoded =
            url::form_urlencoded::byte_serialize(sig_b64.as_bytes()).collect::<String>();

        Ok(format!(
            "SharedAccessSignature sr={}&sig={}&se={}&skn={}",
            encoded_uri, sig_encoded, expiry, self.key_name
        ))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl TokenCredential for EventHubsSasCredential {
    async fn get_token(
        &self,
        _scopes: &[&str],
        _options: Option<TokenRequestOptions<'_>>,
    ) -> azure_core::Result<AccessToken> {
        let token = self.generate_sas_token(3600).map_err(|e| {
            azure_core::error::Error::with_message(
                azure_core::error::ErrorKind::Credential,
                format!("Failed to generate SAS token: {e}"),
            )
        })?;

        let expires_on =
            azure_core::time::OffsetDateTime::now_utc() + std::time::Duration::from_secs(3600);

        Ok(AccessToken::new(token, expires_on))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AzureEventHubsSourceConfig>();
    }

    // --- Connection string parsing ---

    #[test]
    fn parse_full_connection_string() {
        let cs = "Endpoint=sb://mynamespace.servicebus.windows.net/;SharedAccessKeyName=mykeyname;SharedAccessKey=dGVzdGtleQ==;EntityPath=my-hub";
        let parsed = ParsedEventHubsConnectionString::parse(cs).unwrap();
        assert_eq!(parsed.endpoint, "sb://mynamespace.servicebus.windows.net/");
        assert_eq!(parsed.shared_access_key_name, "mykeyname");
        assert_eq!(parsed.shared_access_key, "dGVzdGtleQ==");
        assert_eq!(parsed.entity_path, Some("my-hub".to_string()));
    }

    #[test]
    fn parse_connection_string_without_entity_path() {
        let cs = "Endpoint=sb://ns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc123==";
        let parsed = ParsedEventHubsConnectionString::parse(cs).unwrap();
        assert_eq!(parsed.endpoint, "sb://ns.servicebus.windows.net/");
        assert_eq!(parsed.shared_access_key_name, "key1");
        assert_eq!(parsed.shared_access_key, "abc123==");
        assert!(parsed.entity_path.is_none());
    }

    #[test]
    fn parse_connection_string_with_base64_padding() {
        // SharedAccessKey values often contain '=' from base64 padding
        let cs = "Endpoint=sb://ns.servicebus.windows.net/;SharedAccessKeyName=RootKey;SharedAccessKey=abc+def/ghi123jklMNO456pqr789stu0vwxyz12345A=";
        let parsed = ParsedEventHubsConnectionString::parse(cs).unwrap();
        assert_eq!(
            parsed.shared_access_key,
            "abc+def/ghi123jklMNO456pqr789stu0vwxyz12345A="
        );
    }

    #[test]
    fn parse_connection_string_missing_endpoint() {
        let cs = "SharedAccessKeyName=key1;SharedAccessKey=abc==";
        let result = ParsedEventHubsConnectionString::parse(cs);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing 'Endpoint'")
        );
    }

    #[test]
    fn parse_connection_string_missing_key_name() {
        let cs = "Endpoint=sb://ns.servicebus.windows.net/;SharedAccessKey=abc==";
        let result = ParsedEventHubsConnectionString::parse(cs);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing 'SharedAccessKeyName'")
        );
    }

    #[test]
    fn parse_connection_string_missing_key() {
        let cs = "Endpoint=sb://ns.servicebus.windows.net/;SharedAccessKeyName=key1";
        let result = ParsedEventHubsConnectionString::parse(cs);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing 'SharedAccessKey'")
        );
    }

    #[test]
    fn parse_connection_string_ignores_empty_segments() {
        let cs = "Endpoint=sb://ns.servicebus.windows.net/;;SharedAccessKeyName=key1;;SharedAccessKey=dGVzdA==;;";
        let parsed = ParsedEventHubsConnectionString::parse(cs).unwrap();
        assert_eq!(parsed.endpoint, "sb://ns.servicebus.windows.net/");
        assert_eq!(parsed.shared_access_key_name, "key1");
        assert!(!parsed.use_development_emulator);
    }

    #[test]
    fn parse_connection_string_emulator_mode() {
        let cs = "Endpoint=sb://localhost;SharedAccessKeyName=RootManageSharedAccessKey;SharedAccessKey=SAS_KEY_VALUE;UseDevelopmentEmulator=true";
        let parsed = ParsedEventHubsConnectionString::parse(cs).unwrap();
        assert_eq!(parsed.endpoint, "sb://localhost");
        assert!(parsed.use_development_emulator);
    }

    // --- SAS credential ---

    #[test]
    fn sas_credential_new_valid() {
        let cred = EventHubsSasCredential::new(
            "sb://mynamespace.servicebus.windows.net/",
            "RootManageSharedAccessKey",
            "dGVzdGtleQ==", // base64 of "testkey"
        );
        assert!(cred.is_ok());
        let cred = cred.unwrap();
        assert_eq!(cred.resource_uri, "mynamespace.servicebus.windows.net");
        assert_eq!(cred.key_name, "RootManageSharedAccessKey");
    }

    #[test]
    fn sas_credential_strips_scheme_and_trailing_slash() {
        let cred =
            EventHubsSasCredential::new("sb://myns.servicebus.windows.net/", "key1", "dGVzdA==")
                .unwrap();
        assert_eq!(cred.resource_uri, "myns.servicebus.windows.net");
    }

    #[test]
    fn sas_credential_invalid_base64() {
        let result = EventHubsSasCredential::new(
            "sb://ns.servicebus.windows.net/",
            "key1",
            "not-valid-base64!!!",
        );
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid SharedAccessKey base64")
        );
    }

    #[test]
    fn sas_token_generation() {
        let cred = EventHubsSasCredential::new(
            "sb://mynamespace.servicebus.windows.net/",
            "mykeyname",
            "dGVzdGtleQ==",
        )
        .unwrap();

        let token = cred.generate_sas_token(3600).unwrap();
        assert!(token.starts_with("SharedAccessSignature sr="));
        assert!(token.contains("&sig="));
        assert!(token.contains("&se="));
        assert!(token.contains("&skn=mykeyname"));
    }

    #[tokio::test]
    async fn sas_credential_get_token() {
        let cred = EventHubsSasCredential::new(
            "sb://mynamespace.servicebus.windows.net/",
            "mykeyname",
            "dGVzdGtleQ==",
        )
        .unwrap();

        let token = cred.get_token(&["scope"], None).await.unwrap();
        assert!(token.token.secret().starts_with("SharedAccessSignature"));
    }

    // --- build_credential ---

    #[test]
    fn build_credential_connection_string_with_entity_path() {
        let cs = SensitiveString::from(
            "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=dGVzdA==;EntityPath=my-hub".to_string(),
        );
        let (ns, eh, _cred, custom_ep) = build_credential(Some(&cs), None, None).unwrap();
        assert_eq!(ns, "myns.servicebus.windows.net");
        assert_eq!(eh, "my-hub");
        assert!(custom_ep.is_none());
    }

    #[test]
    fn build_credential_connection_string_override_entity_path() {
        let cs = SensitiveString::from(
            "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=dGVzdA==;EntityPath=from-cs".to_string(),
        );
        let (_, eh, _, _) = build_credential(Some(&cs), None, Some("override-hub")).unwrap();
        assert_eq!(eh, "override-hub");
    }

    #[test]
    fn build_credential_connection_string_missing_entity_path() {
        let cs = SensitiveString::from(
            "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=dGVzdA==".to_string(),
        );
        let result = build_credential(Some(&cs), None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Event Hub name"));
    }

    #[test]
    fn build_credential_identity_missing_namespace() {
        let result = build_credential(None, None, Some("my-hub"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("namespace"));
    }

    #[test]
    fn build_credential_identity_missing_event_hub_name() {
        let result = build_credential(None, Some("ns.servicebus.windows.net"), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("event_hub_name"));
    }

    #[test]
    fn build_credential_emulator_mode() {
        let cs = SensitiveString::from(
            "Endpoint=sb://localhost;SharedAccessKeyName=RootManageSharedAccessKey;SharedAccessKey=SAS_KEY_VALUE;UseDevelopmentEmulator=true;EntityPath=eh1".to_string(),
        );
        let (ns, eh, _cred, custom_ep) = build_credential(Some(&cs), None, None).unwrap();
        assert_eq!(ns, "localhost");
        assert_eq!(eh, "eh1");
        assert_eq!(custom_ep, Some("amqp://localhost:5672".to_string()));
    }

    // --- Source config defaults ---

    #[test]
    fn config_defaults() {
        let config = AzureEventHubsSourceConfig::default();
        assert_eq!(config.consumer_group, "$Default");
        assert_eq!(config.start_position, "latest");
        assert!(config.partition_ids.is_empty());
        assert!(config.connection_string.is_none());
        assert!(config.namespace.is_none());
        assert!(config.event_hub_name.is_none());
    }

    #[test]
    fn config_from_toml_connection_string() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
        "#;
        let config: AzureEventHubsSourceConfig = toml::from_str(toml_str).unwrap();
        assert!(config.connection_string.is_some());
        assert_eq!(config.consumer_group, "$Default");
        assert_eq!(config.start_position, "latest");
    }

    #[test]
    fn config_from_toml_identity_auth() {
        let toml_str = r#"
            namespace = "myns.servicebus.windows.net"
            event_hub_name = "my-hub"
        "#;
        let config: AzureEventHubsSourceConfig = toml::from_str(toml_str).unwrap();
        assert!(config.connection_string.is_none());
        assert_eq!(
            config.namespace.as_deref(),
            Some("myns.servicebus.windows.net")
        );
        assert_eq!(config.event_hub_name.as_deref(), Some("my-hub"));
    }

    #[test]
    fn config_custom_consumer_group_and_partitions() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            consumer_group = "my-cg"
            partition_ids = ["0", "2"]
            start_position = "earliest"
        "#;
        let config: AzureEventHubsSourceConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.consumer_group, "my-cg");
        assert_eq!(config.partition_ids, vec!["0", "2"]);
        assert_eq!(config.start_position, "earliest");
    }

    #[test]
    fn config_rejects_unknown_fields() {
        let toml_str = r#"
            connection_string = "Endpoint=sb://myns.servicebus.windows.net/;SharedAccessKeyName=key1;SharedAccessKey=abc==;EntityPath=my-hub"
            unknown_field = "bad"
        "#;
        let result = toml::from_str::<AzureEventHubsSourceConfig>(toml_str);
        assert!(result.is_err());
    }

    // --- Output schema definition ---

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = AzureEventHubsSourceConfig {
            log_namespace: Some(true),
            ..Default::default()
        };

        let definition = config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        // Vector namespace: event root is bytes, metadata has source fields
        assert!(definition.is_some());
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = AzureEventHubsSourceConfig::default();

        let definition = config
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        assert!(definition.is_some());
    }
}
