use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use async_trait::async_trait;
use futures_util::FutureExt;
use vector_lib::configurable::configurable_component;
use vector_lib::{
    config::LogNamespace,
    event::{Event, LogEvent},
    schema::Definition,
};
use vector_lib::{
    config::{DataType, SourceOutput},
    source::Source,
};

use crate::config::{GenerateConfig, SourceConfig, SourceContext};

/// Configuration for the `test_backpressure` source.
#[configurable_component(source("test_backpressure", "Test (backpressure)."))]
#[derive(Clone, Debug)]
pub struct BackpressureSourceConfig {
    // The number of events that have been sent.
    #[serde(skip)]
    pub counter: Arc<AtomicUsize>,
}

impl GenerateConfig for BackpressureSourceConfig {
    fn generate_config() -> toml::Value {
        let config = Self {
            counter: Arc::new(AtomicUsize::new(0)),
        };
        toml::Value::try_from(&config).unwrap()
    }
}

#[async_trait]
#[typetag::serde(name = "test_backpressure")]
impl SourceConfig for BackpressureSourceConfig {
    async fn build(&self, mut cx: SourceContext) -> crate::Result<Source> {
        let counter = Arc::clone(&self.counter);
        Ok(async move {
            for i in 0.. {
                let _result = cx
                    .out
                    .send_event(Event::Log(LogEvent::from(format!("event-{}", i))))
                    .await;
                counter.fetch_add(1, Ordering::AcqRel);

                // Place ourselves at the back of Tokio's task queue, giving downstream
                // components a chance to process the event we just sent before sending more.
                //
                // This helps the backpressure tests behave more deterministically when we use
                // opportunistic batching at the topology level. Yielding here makes it very
                // unlikely that a `ready_chunks` or similar will have a chance to see more
                // than one event available at a time.
                tokio::task::yield_now().await;
            }
            Ok(())
        }
        .boxed())
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_logs(
            DataType::all(),
            Definition::default_legacy_namespace(),
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}
