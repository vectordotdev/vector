use anyhow::{Context, Result};
use remote_config::{Client, Config};

#[tokio::main]
async fn main() -> Result<()> {
    let api_key = std::env::var("DD_API_KEY").context("missing DD_API_KEY env var")?;
    let app_key = std::env::var("DD_APP_KEY").context("missing DD_APP_KEY env var")?;

    let hostname = String::from("foo");
    let agent_version = String::from("7.38.0-devel+git.58.9cc8e5c");

    let config = Config {
        site: "datad0g.com".into(),
        api_key,
        app_key,
        hostname,
        agent_version,
    };
    let mut client = Client::initialize(config).await?;

    assert!(client
        .available_products()?
        .iter()
        .any(|product| product == "DEBUG"));
    client.add_product("DEBUG");

    client.update().await?;

    for (path, desc) in client.targets()? {
        dbg!(&path, desc);
        let target = client.fetch_target(path).await?;
        dbg!(target);
    }

    Ok(())
}
