use vector_config::configurable_component;

use crate::sinks::util::UriSerde;

/// Default settings to use for Datadog components.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    /// Default Datadog API key to use for Datadog components.
    #[serde(default = "default_api_key")]
    #[derivative(Default(value = "default_api_key()"))]
    pub api_key: Option<String>,
    /// Default site to use for DataDog components.
    #[serde(default = "default_site")]
    #[derivative(Default(value = "default_site()"))]
    pub site: Option<UriSerde>,
}

fn default_api_key() -> Option<String> {
    std::env::var("DD_API_KEY").ok().map(|s| s.to_string())
}

fn default_site() -> Option<UriSerde> {
    std::env::var("DD_SITE").ok().and_then(|s| s.parse().ok())
}
