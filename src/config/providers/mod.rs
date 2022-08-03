use async_trait::async_trait;

use super::ConfigBuilder;
use crate::signal;

mod http;
mod inline;

use inline::InlineProvider;

type Result = std::result::Result<ConfigBuilder, Vec<String>>;

#[async_trait]
#[typetag::serde(tag = "type")]
pub trait ProviderConfig: core::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    /// Builds a provider, returning a string containing the config. It's passed a signals
    /// channel to control reloading and shutdown, as applicable.
    async fn build(&mut self, signal_handler: &mut signal::SignalHandler) -> Result;

    fn provider_type(&self) -> &'static str;
}

dyn_clone::clone_trait_object!(ProviderConfig);

pub fn from_builder(
    builder: &mut ConfigBuilder,
) -> std::result::Result<Box<dyn ProviderConfig>, Vec<String>> {
    let explicit = builder.provider.take();
    let inline = ((builder.sources.len() + builder.transforms.len() + builder.sinks.len()) > 0)
        .then_some(InlineProvider::new(builder.clone()));

    match (explicit, inline) {
        (Some(_), Some(_)) => Err(vec![
            "No sources/transforms/sinks are allowed if provider config is present.".to_owned(),
        ]),
        (Some(x), None) => Ok(x),
        (None, Some(x)) => Ok(x),
        // Fall back to an empty inline builder
        (None, None) => Ok(InlineProvider::new(builder.clone())),
    }
}
