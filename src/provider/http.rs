use crate::config::provider::ProviderDescription;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct HttpConfig {
    host_key: Option<Url>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self { host_key: None }
    }
}

inventory::submit! {
    ProviderDescription::new::<HttpConfig>("http")
}

impl_generate_config_from_default!(HttpConfig);
