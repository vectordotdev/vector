use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use async_trait::async_trait;
use vector_lib::buffers::topology::channel::{limited, LimitedReceiver};
use vector_lib::configurable::configurable_component;
use vector_lib::{config::LogNamespace, schema::Definition};
use vector_lib::{
    config::{DataType, SourceOutput},
    event::EventContainer,
    source::Source,
};

use crate::{
    config::{SourceConfig, SourceContext},
    source_sender::SourceSenderItem,
};

/// Configuration for the `test_basic` source.
#[configurable_component(source("test_basic", "Test (basic)."))]
#[derive(Clone, Debug)]
#[serde(default)]
pub struct BasicSourceConfig {
    #[serde(skip)]
    receiver: Arc<Mutex<Option<LimitedReceiver<SourceSenderItem>>>>,

    #[serde(skip)]
    event_counter: Option<Arc<AtomicUsize>>,

    #[serde(skip)]
    data_type: Option<DataType>,

    #[serde(skip)]
    force_shutdown: bool,

    /// Meaningless field that only exists for triggering config diffs during topology reloading.
    data: Option<String>,
}

impl Default for BasicSourceConfig {
    fn default() -> Self {
        let (_, receiver) = limited(1000);
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: None,
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: None,
        }
    }
}

impl_generate_config_from_default!(BasicSourceConfig);

impl BasicSourceConfig {
    pub fn new(receiver: LimitedReceiver<SourceSenderItem>) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: None,
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: None,
        }
    }

    pub fn new_with_data(receiver: LimitedReceiver<SourceSenderItem>, data: &str) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: None,
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: Some(data.into()),
        }
    }

    pub fn new_with_event_counter(
        receiver: LimitedReceiver<SourceSenderItem>,
        event_counter: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: Some(event_counter),
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: None,
        }
    }

    pub fn set_force_shutdown(&mut self, force_shutdown: bool) {
        self.force_shutdown = force_shutdown;
    }
}

#[async_trait]
#[typetag::serde(name = "test_basic")]
impl SourceConfig for BasicSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let wrapped = Arc::clone(&self.receiver);
        let event_counter = self.event_counter.clone();
        let mut recv = wrapped.lock().unwrap().take().unwrap();
        let shutdown1 = cx.shutdown.clone();
        let shutdown2 = cx.shutdown;
        let mut out = cx.out;
        let force_shutdown = self.force_shutdown;

        Ok(Box::pin(async move {
            tokio::pin!(shutdown1);
            tokio::pin!(shutdown2);

            loop {
                tokio::select! {
                    biased;

                    _ = &mut shutdown1, if force_shutdown => break,

                    Some(array) = recv.next() => {
                        if let Some(counter) = &event_counter {
                            counter.fetch_add(array.len(), Ordering::Relaxed);
                        }

                        if let Err(e) = out.send_event(array).await {
                            error!(message = "Error sending in sink..", %e);
                            return Err(())
                        }
                    },

                    _ = &mut shutdown2, if !force_shutdown => break,
                }
            }

            info!("Finished sending.");
            Ok(())
        }))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_logs(
            self.data_type.unwrap(),
            Definition::default_legacy_namespace(),
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}
