use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use vector_buffers::topology::channel::{limited, LimitedReceiver};
use vector_core::{
    config::{DataType, Output},
    event::{EventArray, EventContainer},
    source::Source,
};

use crate::config::{SourceConfig, SourceContext, SourceDescription};

/// A test source.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct BasicSourceConfig {
    #[serde(skip)]
    receiver: Arc<Mutex<Option<LimitedReceiver<EventArray>>>>,
    #[serde(skip)]
    event_counter: Option<Arc<AtomicUsize>>,
    #[serde(skip)]
    data_type: Option<DataType>,
    #[serde(skip)]
    force_shutdown: bool,
    // something for serde to use, so we can trigger rebuilds
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

inventory::submit! {
    SourceDescription::new::<BasicSourceConfig>("basic_source")
}

impl BasicSourceConfig {
    pub fn new(receiver: LimitedReceiver<EventArray>) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: None,
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: None,
        }
    }

    pub fn new_with_data(receiver: LimitedReceiver<EventArray>, data: &str) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            event_counter: None,
            data_type: Some(DataType::all()),
            force_shutdown: false,
            data: Some(data.into()),
        }
    }

    pub fn new_with_event_counter(
        receiver: LimitedReceiver<EventArray>,
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
#[typetag::serde(name = "basic_source")]
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

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(self.data_type.unwrap())]
    }

    fn source_type(&self) -> &'static str {
        "basic_source"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}
