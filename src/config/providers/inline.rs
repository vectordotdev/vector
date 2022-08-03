use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{ConfigBuilder, ProviderConfig, Result};
use crate::signal;

/// A provider that simply returns the sources/transforms/sinks defined directly in a normal config
/// file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InlineProvider(ConfigBuilder);

impl InlineProvider {
    pub fn new(builder: ConfigBuilder) -> Box<dyn ProviderConfig> {
        Box::new(Self(builder))
    }
}

#[async_trait]
#[typetag::serde(name = "inline")]
impl ProviderConfig for InlineProvider {
    async fn build(&mut self, _signal_handler: &mut signal::SignalHandler) -> Result {
        Ok(self.0.clone())
    }

    fn provider_type(&self) -> &'static str {
        "inline"
    }
}
