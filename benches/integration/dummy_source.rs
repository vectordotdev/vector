use bytes::Bytes;
use derivative::Derivative;
use serde_with::serde_as;
use std::num::NonZeroUsize;
use std::panic;
use std::sync::Arc;

use tokio::sync::Barrier;
use tokio::{pin, select};

use vector::codecs::Decoder;
use vector::codecs::DecodingConfig;
use vector::config::{LogNamespace, SourceConfig, SourceContext, SourceOutput};
use vector::event::Event;
use vector::shutdown::ShutdownSignal;
use vector::sinks::prelude::FutureExt;
use vector::sources::Source;
use vector::SourceSender;

use vector_lib::codecs::decoding::{DeserializerConfig, FramingConfig};
use vector_lib::codecs::BytesDeserializerConfig;
use vector_lib::configurable::configurable_component;
use vector_lib::impl_generate_config_from_default;

use vector_lib::Result as VectorResult;

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

        let source = DummySource {
            concurrency: self.client_concurrency.into(),
            batch_count: self.batch_count.into(),
            batch_size: self.batch_size.into(),
            message_content,
            decoder,
            barrier,
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
        false
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
}

impl DummySource {
    pub async fn run(self, out: SourceSender, shutdown: ShutdownSignal) -> Result<(), ()> {
        let mut task_handles: Vec<_> = (0..self.concurrency)
            .map(|_| {
                let source = self.clone();
                let shutdown = shutdown.clone().fuse();
                let mut out = out.clone();
                let barrier = self.barrier.clone();

                tokio::spawn(async move {
                    pin!(shutdown);
                    barrier.worker_wait().await;
                    for _ in 0..source.batch_count {
                        select! {
                            _ = &mut shutdown => break,
                            _ = source.send_single_batch(&mut out) => {},
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
        // finalizer: Option<&Arc<Finalizer>>,
        // events_received: Registered<EventsReceived>,
    ) {
        let events: Vec<Event> = (0..self.batch_size)
            .flat_map(|_| {
                let (event, _) = self
                    .decoder
                    .deserializer_parse(self.message_content.clone())
                    .unwrap();
                event
            })
            .collect();
        out.send_batch(events.into_iter()).await.unwrap();
    }
}
