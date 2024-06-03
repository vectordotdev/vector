use std::{future::Future, pin::Pin, time::Duration};

use async_stream::stream;
use bytes::Bytes;
use futures::{stream::StreamExt, Stream};
use tokio::{select, time};
use tokio_stream::wrappers::IntervalStream;
use vrl::path;

use vector_lib::{
    codecs::{
        decoding::{DeserializerConfig, FramingConfig, NewlineDelimitedDecoderOptions},
        NewlineDelimitedDecoderConfig,
    },
    config::LegacyKey,
    internal_event::{ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol},
    sensitive_string::SensitiveString,
};

use crate::{
    azure::ClientCredentials,
    codecs::{Decoder, DecodingConfig},
    config::{
        LogNamespace, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
    },
    event::{BatchNotifier, BatchStatus, EstimatedJsonEncodedSizeOf, Event},
    internal_events::{EventsReceived, StreamClosedError},
    serde::{bool_or_struct, default_decoding},
    shutdown::ShutdownSignal,
    sinks::prelude::configurable_component,
    sources::azure_blob::queue::make_azure_row_stream,
    SourceSender,
};

#[cfg(all(test, feature = "azure-blob-source-integration-tests"))]
mod integration_tests;
pub mod queue;

/// Strategies for consuming objects from Azure Storage.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
enum Strategy {
    /// Consumes objects by processing events sent to an [Azure Storage Queue][azure_storage_queue].
    ///
    /// [azure_storage_queue]: https://learn.microsoft.com/en-us/azure/storage/queues/storage-queues-introduction
    StorageQueue,

    /// This is a test strategy used only of development and PoC. Should be removed
    /// once development is done.
    #[derivative(Default)]
    Test,
}

/// WIP
/// A dummy implementation is used as a starter.
/// The source will send dummy messages at a fixed interval, incrementing a counter every
/// exec_interval_secs seconds.
#[configurable_component(source("azure_blob", "Collect logs from Azure Container."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(default, deny_unknown_fields)]
pub struct AzureBlobConfig {
    // TODO : everything
    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    /// The interval, in seconds, between subsequent dummy messages
    #[serde(default = "default_exec_interval_secs")]
    exec_interval_secs: u64,

    /// The strategy to use to consume objects from Azure Storage.
    #[configurable(metadata(docs::hidden))]
    strategy: Strategy,

    /// Configuration options for Storage Queue.
    queue: Option<queue::Config>,

    /// The Azure Blob Storage Account connection string.
    ///
    /// Authentication with access key is the only supported authentication method.
    ///
    /// Either `storage_account`, or this field, must be specified.
    #[configurable(metadata(
        docs::examples = "DefaultEndpointsProtocol=https;AccountName=mylogstorage;AccountKey=storageaccountkeybase64encoded;EndpointSuffix=core.windows.net"
    ))]
    pub connection_string: Option<SensitiveString>,

    /// The Azure Blob Storage Account name.
    ///
    /// Attempts to load credentials for the account in the following ways, in order:
    ///
    /// - read from environment variables ([more information][env_cred_docs])
    /// - looks for a [Managed Identity][managed_ident_docs]
    /// - uses the `az` CLI tool to get an access token ([more information][az_cli_docs])
    ///
    /// Either `connection_string`, or this field, must be specified.
    ///
    /// [env_cred_docs]: https://docs.rs/azure_identity/latest/azure_identity/struct.EnvironmentCredential.html
    /// [managed_ident_docs]: https://docs.microsoft.com/en-us/azure/active-directory/managed-identities-azure-resources/overview
    /// [az_cli_docs]: https://docs.microsoft.com/en-us/cli/azure/account?view=azure-cli-latest#az-account-get-access-token
    #[configurable(metadata(docs::examples = "mylogstorage"))]
    pub storage_account: Option<String>,

    #[configurable(derived)]
    pub client_credentials: Option<ClientCredentials>,

    /// The Azure Blob Storage Endpoint URL.
    ///
    /// This is used to override the default blob storage endpoint URL in cases where you are using
    /// credentials read from the environment/managed identities or access tokens without using an
    /// explicit connection_string (which already explicitly supports overriding the blob endpoint
    /// URL).
    ///
    /// This may only be used with `storage_account` and is ignored when used with
    /// `connection_string`.
    #[configurable(metadata(docs::examples = "https://test.blob.core.usgovcloudapi.net/"))]
    #[configurable(metadata(docs::examples = "https://test.blob.core.windows.net/"))]
    pub endpoint: Option<String>,

    /// The Azure Blob Storage Account container name.
    #[configurable(metadata(docs::examples = "my-logs"))]
    pub(super) container_name: String,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    pub acknowledgements: SourceAcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,
}

impl_generate_config_from_default!(AzureBlobConfig);

impl AzureBlobConfig {
    /// Self validation
    pub fn validate(&self) -> crate::Result<()> {
        match self.strategy {
            Strategy::StorageQueue => {
                if self.queue.is_none() || self.queue.as_ref().unwrap().queue_name.is_empty() {
                    return Err("Azure event grid queue must be set.".into());
                }
                if self.storage_account.clone().unwrap_or_default().is_empty()
                    && self
                        .connection_string
                        .clone()
                        .unwrap_or_default()
                        .inner()
                        .is_empty()
                {
                    return Err("Azure Storage Account or Connection String must be set.".into());
                }
                if self.container_name.is_empty() {
                    return Err("Azure Container must be set.".into());
                }
            }
            Strategy::Test => {
                if self.exec_interval_secs == 0 {
                    return Err("exec_interval_secs must be greater than 0".into());
                }
            }
        }

        Ok(())
    }
}

type BlobStream = Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>;

pub struct BlobPack {
    row_stream: BlobStream,
    success_handler: Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>,
}
type BlobPackStream = Pin<Box<dyn Stream<Item = BlobPack> + Send>>;

async fn run_streaming(
    config: AzureBlobConfig,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
    log_namespace: LogNamespace,
    mut blob_stream: BlobPackStream,
    acknowledge: bool, // TODO: use proper enum
) -> Result<(), ()> {
    debug!("Starting Azure streaming.");

    let framing = FramingConfig::NewlineDelimited(NewlineDelimitedDecoderConfig {
        newline_delimited: NewlineDelimitedDecoderOptions { max_length: None },
    });
    let decoder_base = DecodingConfig::new(framing, config.decoding.clone(), log_namespace)
        .build()
        .map_err(|e| {
            error!("Failed to build decoder: {}", e);
            ()
        })?;

    loop {
        select! {
            blob_pack = blob_stream.next() => {
                match blob_pack{
                    Some(blob_pack) => {
                        process_blob_pack(blob_pack, &mut out, &log_namespace, decoder_base.clone(), acknowledge).await?;
                    }
                    None => {
                        break; // end of stream
                    }
                }
            },
            _ = shutdown.clone() => {
                break;
            }
        }
    }

    Ok(())
}

async fn process_blob_pack(
    blob_pack: BlobPack,
    out: &mut SourceSender,
    log_namespace: &LogNamespace,
    decoder: Decoder,
    acknowledge: bool,
) -> Result<(), ()> {
    let events_received = register!(EventsReceived);
    events_received.emit(CountByteSize(1, 1.into()));
    let bytes_received = register!(BytesReceived::from(Protocol::HTTP));
    let (batch, receiver) = BatchNotifier::maybe_new_with_receiver(acknowledge);
    let mut row_stream = blob_pack.row_stream;
    let mut output_stream = stream! {
        // TODO: consider selecting with a shutdown
        while let Some(row) = row_stream.next().await {
            bytes_received.emit(ByteSize(row.len()));
            let deser_result = decoder.deserializer_parse(Bytes::from(row));
            if deser_result.is_err(){
                continue;
            }
            let (events, _) = deser_result.unwrap();
            for mut event in events.into_iter(){
                event = event.with_batch_notifier_option(&batch);
                match event {
                    Event::Log(ref mut log_event) => {
                        log_namespace.insert_source_metadata(
                            AzureBlobConfig::NAME,
                            log_event,
                            Some(LegacyKey::Overwrite("ingest_timestamp")),
                            path!("ingest_timestamp"),
                            chrono::Utc::now().to_rfc3339(),
                        );
                        events_received.emit(CountByteSize(1, event.estimated_json_encoded_size_of()));
                        yield event
                    }
                    _ => {
                        error!("Expected Azure rows as Log Events, but got {:?}.", event);
                    }
                }
            }

        }
        // Explicitly dropping to showcase that the status of the batch is sent to the channel.
        drop(batch);
    }
    .boxed();

    // Return if send was unsuccessful.
    if let Err(send_error) = out.send_event_stream(&mut output_stream).await {
        // TODO: consider dedicated error.
        error!("Failed to send event stream: {}.", send_error);
        let (count, _) = output_stream.size_hint();
        emit!(StreamClosedError { count });
        return Ok(());
    }

    // dropping like s3 sender
    drop(output_stream); // TODO: better explanation

    // Run success handler if there are no errors in send or acknowledgement.
    match receiver {
        None => (blob_pack.success_handler)().await,
        Some(receiver) => {
            let result = receiver.await;
            match result {
                BatchStatus::Delivered => (blob_pack.success_handler)().await, // TODO: emit
                BatchStatus::Errored => {
                    // TODO: emit a proper error
                    error!("Batch event had a transient error in delivery.")
                }
                BatchStatus::Rejected => {
                    // TODO: emit a proper error
                    // TODO: consider allowing rejected events wihtout retrying, like s3
                    error!("Batch event had a permanent failure or rejection.")
                }
            }
        }
    }

    Ok(())
}

#[async_trait::async_trait]
#[typetag::serde(name = "azure_blob")]
impl SourceConfig for AzureBlobConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        self.validate()?;
        let log_namespace = cx.log_namespace(self.log_namespace);
        let self_clone = self.clone();
        let acknowledgments = cx.do_acknowledgements(self.acknowledgements);

        let blob_pack_stream: BlobPackStream = match self.strategy {
            Strategy::Test => {
                // streaming incremented numbers periodically
                let exec_interval_secs = self.exec_interval_secs;
                let shutdown = cx.shutdown.clone();
                stream! {
                    let schedule = Duration::from_secs(exec_interval_secs);
                    let mut counter = 0;
                    let mut interval = IntervalStream::new(time::interval(schedule)).take_until(shutdown);
                    while interval.next().await.is_some() {
                        counter += 1;
                        let counter_copy = counter;
                        yield BlobPack {
                            row_stream: stream! {
                                for i in 0..=counter {
                                    yield format!("{}:{}", counter, i).into_bytes();
                                }
                            }.boxed(),
                            success_handler: Box::new(move || {
                                Box::pin(async move {
                                    debug!("Successfully processed blob pack for counter {}.", counter_copy);
                                })
                            }),
                        }
                    }
                }.boxed()
            }
            Strategy::StorageQueue => make_azure_row_stream(self)?,
        };
        Ok(Box::pin(run_streaming(
            self_clone,
            cx.shutdown,
            cx.out.clone(),
            log_namespace,
            blob_pack_stream,
            acknowledgments,
        )))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();

        vec![SourceOutput::new_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

fn default_exec_interval_secs() -> u64 {
    1
}
