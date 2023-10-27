//! Datadog global options.
//! This is used to allow settings (api_key and site) to be specified in the configuration file
//! globally which will apply to all datadog components. Each component can override the settings
//! specified here if necessary.
//!
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};

use crate::common::datadog::DD_US_SITE;

/// Default settings to use for Datadog components.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    /// Default Datadog API key to use for Datadog components.
    ///
    /// This can also be specified with the `DD_API_KEY` environment variable.
    #[derivative(Default(value = "default_api_key()"))]
    #[configurable(metadata(docs::examples = "${DATADOG_API_KEY_ENV_VAR}"))]
    #[configurable(metadata(docs::examples = "ef8d5de700e7989468166c40fc8a0ccd"))]
    pub api_key: Option<SensitiveString>,

    /// Default site to use for Datadog components.
    ///
    /// This can also be specified with the `DD_SITE` environment variable.
    #[serde(default = "default_site")]
    #[derivative(Default(value = "default_site()"))]
    #[configurable(metadata(docs::examples = "us3.datadoghq.com"))]
    #[configurable(metadata(docs::examples = "datadoghq.eu"))]
    pub site: String,
}

fn default_api_key() -> Option<SensitiveString> {
    std::env::var("DD_API_KEY").ok().map(Into::into)
}

pub fn default_site() -> String {
    std::env::var("DD_SITE").unwrap_or(DD_US_SITE.to_string())
}
