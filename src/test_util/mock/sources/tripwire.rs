use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_util::{future, FutureExt};
use stream_cancel::{Trigger, Tripwire};
use vector_lib::config::LogNamespace;
use vector_lib::configurable::configurable_component;
use vector_lib::schema::Definition;
use vector_lib::{
    config::{DataType, SourceOutput},
    source::Source,
};

use crate::config::{GenerateConfig, SourceConfig, SourceContext};

/// Configuration for the `test_tripwire` source.
#[configurable_component(source("test_tripwire", "Test (tripwire)."))]
#[derive(Clone, Debug)]
pub struct TripwireSourceConfig {
    #[serde(skip)]
    tripwire: Arc<Mutex<Option<Tripwire>>>,
}

impl GenerateConfig for TripwireSourceConfig {
    fn generate_config() -> toml::Value {
        let config = Self {
            tripwire: Arc::new(Mutex::new(None)),
        };
        toml::Value::try_from(&config).unwrap()
    }
}

impl TripwireSourceConfig {
    pub fn new() -> (Trigger, Self) {
        let (trigger, tripwire) = Tripwire::new();
        (
            trigger,
            Self {
                tripwire: Arc::new(Mutex::new(Some(tripwire))),
            },
        )
    }
}

#[async_trait]
#[typetag::serde(name = "test_tripwire")]
impl SourceConfig for TripwireSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let tripwire = self
            .tripwire
            .lock()
            .expect("who cares if the lock is poisoned");

        let out = cx.out;
        Ok(Box::pin(
            future::select(
                cx.shutdown.map(|_| ()).boxed(),
                tripwire
                    .clone()
                    .unwrap()
                    .then(crate::shutdown::tripwire_handler)
                    .boxed(),
            )
            .map(|_| drop(out))
            .unit_error(),
        ))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            Definition::default_legacy_namespace(),
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}
