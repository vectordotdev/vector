use async_trait::async_trait;
use vector_lib::{
    config::{DataType, LogNamespace, SourceOutput},
    configurable::configurable_component,
    schema::Definition,
    source::Source,
};

use crate::config::{SourceConfig, SourceContext};

/// Configuration for the `test_panic` source.
#[configurable_component(source("test_panic", "Test (panic)."))]
#[derive(Clone, Debug, Default)]
pub struct PanicSourceConfig {
    /// Meaningless field that only exists for triggering config diffs during topology reloading.
    data: Option<String>,
}

impl_generate_config_from_default!(PanicSourceConfig);

#[async_trait]
#[typetag::serde(name = "test_panic")]
impl SourceConfig for PanicSourceConfig {
    async fn build(&self, _cx: SourceContext) -> crate::Result<Source> {
        Ok(Box::pin(async { panic!() }))
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
