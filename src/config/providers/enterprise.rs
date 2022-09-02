use async_stream::stream;
use futures::Stream;
use remote_config::{Client, Config, TargetPath};
use serde::{Deserialize, Serialize};
use tokio::time;

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
            agent_version: crate::get_version(),
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
    async fn build(&mut self, signal_handler: &mut signal::SignalHandler) -> Result {
        let mut client = Client::initialize(self.client_config.clone())
            .await
            .map_err(map_err)?;

        // TODO: use actual Obs Pipelines product when ready
        client.add_product("DEBUG");
        client.update().await.map_err(map_err)?;

        let targets = client.targets().map_err(map_err)?;
        if targets.is_empty() {
            return Err(vec![String::from("no remote config targets available")]);
        }

        // TODO: decide how to pick a target
        let path = targets.keys().next().expect("targets is not empty").clone();
        let version = client.target_version(&path).expect("should have version");

        let builder = load(&mut client, &path).await?;

        signal_handler.add(poll(client, path, version));

        Ok(builder)
    }

    fn provider_type(&self) -> &'static str {
        "enterprise"
    }
}

async fn load(client: &mut Client, path: &TargetPath) -> Result {
    let (builder, warnings) = crate::config::load(
        std::io::Cursor::new(client.fetch_target(&path).await.map_err(map_err)?),
        crate::config::format::Format::Toml,
    )?;
    for warning in warnings.into_iter() {
        warn!("{}", warning);
    }
    Ok(builder)
}

fn poll(
    mut client: Client,
    path: TargetPath,
    mut version: u64,
) -> impl Stream<Item = signal::SignalTo> {
    let duration = time::Duration::from_secs(1);
    let mut interval = time::interval_at(time::Instant::now() + duration, duration);

    stream! {
        loop {
            interval.tick().await;

            match client.update().await {
                Ok(()) => {
                    let new_version = client.target_version(&path).expect("should have version");
                    if new_version > version {
                        info!(message = "new config available", %version);
                        version = new_version;
                        match load(&mut client, &path).await {
                            Ok(builder) => yield signal::SignalTo::ReloadFromConfigBuilder(builder),
                            Err(errors) => for error in errors {
                                error!(?error);
                            }
                        }
                    }
                },
                Err(error) => error!(%error),
            }
        }
    }
}

fn map_err(e: remote_config::Error) -> Vec<String> {
    vec![e.to_string()]
}
