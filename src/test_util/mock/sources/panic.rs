use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use vector_core::config::LogNamespace;
use vector_core::{
    config::{DataType, Output},
    source::Source,
};

use crate::config::{SourceConfig, SourceContext, SourceDescription};

/// A test source that immediately panics.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PanicSourceConfig {
    dummy: Option<String>,
}

impl_generate_config_from_default!(PanicSourceConfig);

inventory::submit! {
    SourceDescription::new::<PanicSourceConfig>("panic_source")
}

#[async_trait]
#[typetag::serde(name = "panic_source")]
impl SourceConfig for PanicSourceConfig {
    async fn build(&self, _cx: SourceContext) -> crate::Result<Source> {
        Ok(Box::pin(async { panic!() }))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "panic_source"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}
