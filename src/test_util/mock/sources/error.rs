use async_trait::async_trait;
use futures_util::{future::err, FutureExt};
use vector_lib::config::LogNamespace;
use vector_lib::configurable::configurable_component;
use vector_lib::schema::Definition;
use vector_lib::{
    config::{DataType, SourceOutput},
    source::Source,
};

use crate::config::{SourceConfig, SourceContext};

/// Configuration for the `test_error` source.
#[configurable_component(source("test_error", "Test (error)."))]
#[derive(Clone, Debug, Default)]
pub struct ErrorSourceConfig {
    /// Meaningless field that only exists for triggering config diffs during topology reloading.
    data: Option<String>,
}

impl_generate_config_from_default!(ErrorSourceConfig);

#[async_trait]
#[typetag::serde(name = "test_error")]
impl SourceConfig for ErrorSourceConfig {
    async fn build(&self, _cx: SourceContext) -> crate::Result<Source> {
        Ok(err(()).boxed())
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
