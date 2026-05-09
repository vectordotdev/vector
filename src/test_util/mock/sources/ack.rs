use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use vector_lib::{
    buffers::{
        config::MemoryBufferSize,
        topology::channel::{LimitedReceiver, limited},
    },
    config::{DataType, LogNamespace, SourceOutput},
    configurable::configurable_component,
    schema::Definition,
    source::Source,
    source_sender::SourceSenderItem,
};

use crate::config::{SourceConfig, SourceContext};

/// Configuration for the `test_ack` source.
///
/// Identical to `BasicSourceConfig` but returns `can_acknowledge() -> true`,
/// enabling the topology to propagate sink acknowledgement requirements back
/// to this source. Tests attach `BatchNotifier` to events before sending them
/// into this source's channel to observe end-to-end ack behavior.
#[configurable_component(source("test_ack", "Test (ack-aware)."))]
#[derive(Clone, Debug)]
#[serde(default)]
pub struct AckSourceConfig {
    #[serde(skip)]
    receiver: Arc<Mutex<Option<LimitedReceiver<SourceSenderItem>>>>,

    #[serde(skip)]
    data_type: Option<DataType>,
}

impl Default for AckSourceConfig {
    fn default() -> Self {
        let limit = MemoryBufferSize::MaxEvents(NonZeroUsize::new(1000).unwrap());
        let (_, receiver) = limited(limit, None, None);
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            data_type: Some(DataType::all_bits()),
        }
    }
}

impl_generate_config_from_default!(AckSourceConfig);

impl AckSourceConfig {
    pub fn new(receiver: LimitedReceiver<SourceSenderItem>) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            data_type: Some(DataType::all_bits()),
        }
    }
}

#[async_trait]
#[typetag::serde(name = "test_ack")]
impl SourceConfig for AckSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let wrapped = Arc::clone(&self.receiver);
        let mut recv = wrapped.lock().unwrap().take().unwrap();
        let shutdown = cx.shutdown;
        let mut out = cx.out;

        Ok(Box::pin(async move {
            tokio::pin!(shutdown);

            loop {
                tokio::select! {
                    biased;

                    Some(array) = recv.next() => {
                        if let Err(e) = out.send_event(array).await {
                            error!(message = "Error sending in ack source.", %e);
                            return Err(())
                        }
                    },

                    _ = &mut shutdown => break,
                }
            }

            info!("Ack source finished.");
            Ok(())
        }))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_maybe_logs(
            self.data_type.unwrap(),
            Definition::default_legacy_namespace(),
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}
