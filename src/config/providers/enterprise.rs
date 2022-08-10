use remote_config::{Client, Config};
use serde::{Deserialize, Serialize};

use super::{ProviderConfig, Result};
use crate::{config::enterprise, signal};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnterpriseProvider {
    client_config: Config,
    config_key: String,
}

pub fn from_opts(opts: &enterprise::Options) -> Option<Box<dyn ProviderConfig>> {
    if opts.enable_remote_config {
        let client_config = Config {
            site: opts.site.clone().unwrap_or("datad0g.com".into()),
            api_key: opts.api_key.clone().expect("need api key"),
            app_key: opts.application_key.clone(),
            hostname: "foo".into(),
            agent_version: "bar".into(),
        };
        Some(Box::new(EnterpriseProvider {
            client_config,
            config_key: opts.configuration_key.clone(),
        }))
    } else {
        None
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "enterprise")]
impl ProviderConfig for EnterpriseProvider {
    async fn build(&mut self, _signal_handler: &mut signal::SignalHandler) -> Result {
        let mut client = Client::initialize(self.client_config.clone())
            .await
            .unwrap();

        client.add_product("DEBUG");
        client.update().await.unwrap();

        let (config_builder, warnings) = crate::config::load(
            std::io::Cursor::new(client.target_files.values().next().unwrap()),
            crate::config::format::Format::Toml,
        )?;

        for warning in warnings.into_iter() {
            warn!("{}", warning);
        }
        Ok(config_builder)
    }

    fn provider_type(&self) -> &'static str {
        "enterprise"
    }
}
