use bytes::Bytes;
use derivative::Derivative;
use futures_util::StreamExt;
use serde_with::serde_as;
use std::num::NonZeroUsize;
use std::panic;
use std::sync::Arc;

use tokio::sync::Barrier;
use tokio::{pin, select};
use tracing::info;
use tracing_futures::Instrument;

use vector::codecs::Decoder;
use vector::codecs::DecodingConfig;
use vector::config::{
    LogNamespace, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
};
use vector::event::{BatchNotifier, BatchStatus, EstimatedJsonEncodedSizeOf};
use vector::internal_events::{EventsReceived, StreamClosedError};
use vector::shutdown::ShutdownSignal;
use vector::sinks::prelude::{CountByteSize, FutureExt};
use vector::sources::Source;
use vector::SourceSender;

use vector_lib::codecs::decoding::{DeserializerConfig, FramingConfig};
use vector_lib::codecs::BytesDeserializerConfig;
use vector_lib::configurable::configurable_component;
use vector_lib::{emit, impl_generate_config_from_default, register};

use vector::serde::bool_or_struct;
use vector_lib::Result as VectorResult;

use vector_lib::finalizer::UnorderedFinalizer;
use vector_lib::internal_event::{InternalEventHandle, Registered};

#[derive(Clone)]
pub struct StartBarrier {
    ready_barrier: Arc<Barrier>,
    start_barrier: Arc<Barrier>,
}

impl StartBarrier {
    pub fn new(workers: usize) -> Self {
        Self {
            ready_barrier: Arc::new(Barrier::new(workers + 1)),
            start_barrier: Arc::new(Barrier::new(workers + 1)),
        }
    }

    pub async fn worker_wait(&self) {
        self.wait_ready().await;
        self.wait_start().await;
    }

    pub async fn wait_ready(&self) {
        self.ready_barrier.wait().await;
    }

    pub async fn wait_start(&self) {
        self.start_barrier.wait().await;
    }
}

pub fn default_decoding() -> DeserializerConfig {
    BytesDeserializerConfig::new().into()
}

fn default_client_concurrency() -> NonZeroUsize {
    NonZeroUsize::new(1).unwrap()
}

fn default_batch_size() -> NonZeroUsize {
    NonZeroUsize::new(10).unwrap()
}

fn default_batch_count() -> NonZeroUsize {
    NonZeroUsize::new(10).unwrap()
}

fn default_message_size() -> NonZeroUsize {
    NonZeroUsize::new(1024).unwrap()
}

/// FooBar
#[serde_as]
#[configurable_component(source("dummy_source", "Generate dummy output"))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
pub struct DummySourceConfig {
    /// Number of tasks
    #[serde(default = "default_client_concurrency")]
    #[derivative(Default(value = "default_client_concurrency()"))]
    pub client_concurrency: NonZeroUsize,

    /// Number of message batches each task will send
    #[serde(default = "default_batch_count")]
    #[derivative(Default(value = "default_batch_count()"))]
    pub batch_count: NonZeroUsize,

    /// Size of each batch
    #[serde(default = "default_batch_size")]
    #[derivative(Default(value = "default_batch_size()"))]
    pub batch_size: NonZeroUsize,

    /// Size of each message
    #[serde(default = "default_message_size")]
    #[derivative(Default(value = "default_message_size()"))]
    pub message_size: NonZeroUsize,

    #[configurable(derived)]
    #[derivative(Default(value = "default_decoding()"))]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    pub acknowledgements: SourceAcknowledgementsConfig,
}

impl_generate_config_from_default!(DummySourceConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "dummy_source")]
impl SourceConfig for DummySourceConfig {
    async fn build(&self, cx: SourceContext) -> VectorResult<Source> {
        let log_namespace = cx.log_namespace(None);

        let message_content: Bytes = (0..self.message_size.into())
            .map(|_| rand::random::<u8>())
            .collect();

        let decoder =
            DecodingConfig::new(FramingConfig::Bytes, self.decoding.clone(), log_namespace)
                .build()?;

        let barrier: StartBarrier = cx.extra_context.get().cloned().unwrap();

        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        let source = DummySource {
            concurrency: self.client_concurrency.into(),
            batch_count: self.batch_count.into(),
            batch_size: self.batch_size.into(),
            message_content,
            decoder,
            barrier,
            acknowledgements,
        };

        Ok(Box::pin(source.run(cx.out, cx.shutdown)))
    }
    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let schema_definition = self.decoding.schema_definition(global_log_namespace);

        vec![SourceOutput::new_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct DummySource {
    pub concurrency: usize,
    pub batch_count: usize,
    pub batch_size: usize,
    pub message_content: Bytes,
    pub decoder: Decoder,
    pub barrier: StartBarrier,
    pub acknowledgements: bool,
}

type Finalizer = UnorderedFinalizer<Vec<usize>>;

impl DummySource {
    pub async fn run(self, out: SourceSender, shutdown: ShutdownSignal) -> Result<(), ()> {
        let finalizer = self.acknowledgements.then(|| {
            let (finalizer, mut ack_stream) = Finalizer::new(Some(shutdown.clone()));
            tokio::spawn(
                async move {
                    let mut total_delivered = 0;
                    while let Some((status, receipts)) = ack_stream.next().await {
                        if status == BatchStatus::Delivered {
                            total_delivered += receipts.len();
                        }
                    }
                    info!("Batch receiver shutdown");
                    total_delivered
                }
                .in_current_span(),
            );
            Arc::new(finalizer)
        });
        let events_received = register!(EventsReceived);

        let mut task_handles: Vec<_> = (0..self.concurrency)
            .map(|_| {
                let source = self.clone();
                let shutdown = shutdown.clone().fuse();
                let mut out = out.clone();
                let barrier = self.barrier.clone();
                let finalizer = finalizer.clone();
                let events_received = events_received.clone();

                tokio::spawn(async move {
                    pin!(shutdown);
                    barrier.worker_wait().await;
                    let finalizer = finalizer.as_ref();
                    for _ in 0..source.batch_count {
                        select! {
                            _ = &mut shutdown => break,
                            _ = source.send_single_batch(&mut out, finalizer, events_received.clone()) => {},
                        }
                    }
                })
            })
            .collect();
        for task_handle in task_handles.drain(..) {
            if let Err(e) = task_handle.await {
                if e.is_panic() {
                    panic::resume_unwind(e.into_panic());
                }
            }
        }
        Ok(())
    }

    async fn send_single_batch(
        &self,
        out: &mut SourceSender,
        finalizer: Option<&Arc<Finalizer>>,
        events_received: Registered<EventsReceived>,
    ) {
        let batch_range = 0..self.batch_size;
        let count = batch_range.len();
        let mut receipts_to_ack = Vec::with_capacity(count);
        let mut events = Vec::with_capacity(count);

        let (batch, batch_receiver) = BatchNotifier::maybe_new_with_receiver(finalizer.is_some());

        for idx in batch_range {
            receipts_to_ack.push(idx);
            let (event, _) = self
                .decoder
                .deserializer_parse(self.message_content.clone())
                .unwrap();
            events_received.emit(CountByteSize(1, event.estimated_json_encoded_size_of()));
            events.extend(
                event
                    .into_iter()
                    .map(|event| event.with_batch_notifier_option(&batch)),
            );
        }

        drop(batch); // Drop last reference to batch acknowledgement finalizer

        match out.send_batch(events.into_iter()).await {
            Ok(()) => {
                if let Some(receiver) = batch_receiver {
                    finalizer
                        .expect("No finalizer")
                        .add(receipts_to_ack, receiver);
                }
            }
            Err(_) => emit!(StreamClosedError { count }),
        }
    }
}
