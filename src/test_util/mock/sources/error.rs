use async_trait::async_trait;
use futures_util::{future::err, FutureExt};
use serde::{Deserialize, Serialize};
use vector_core::config::LogNamespace;
use vector_core::{
    config::{DataType, Output},
    source::Source,
};

use crate::config::{SourceConfig, SourceContext, SourceDescription};

/// A test source that immediately returns an error.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ErrorSourceConfig {
    dummy: Option<String>,
}

impl_generate_config_from_default!(ErrorSourceConfig);

inventory::submit! {
    SourceDescription::new::<ErrorSourceConfig>("error_source")
}

#[async_trait]
#[typetag::serde(name = "error_source")]
impl SourceConfig for ErrorSourceConfig {
    async fn build(&self, _cx: SourceContext) -> crate::Result<Source> {
        Ok(err(()).boxed())
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "error_source"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}
