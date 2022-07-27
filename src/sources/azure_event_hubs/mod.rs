mod config;

pub use config::AzureEventHubsConfig;

use crate::config::SourceDescription;

inventory::submit! {
    SourceDescription::new::<AzureEventHubsConfig>("azure_event_hubs")
}