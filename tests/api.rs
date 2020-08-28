mod support;

#[cfg(feature = "api")]
mod tests {
    use crate::support::{sink, source};
    use vector::config::Config;
    use vector::test_util::{next_addr, start_topology, wait_for_tcp_in_secs};

    fn api_enabled_config() -> Config {
        let mut config = Config::empty();
        config.add_source("in1", source().1);
        config.add_sink("out1", &["in1"], sink(10).1);
        config.api.enabled = true;
        config.api.bind = Some(next_addr());

        config
    }

    #[tokio::test]
    async fn api_config() {
        let config = api_enabled_config();
        let addr = config.api.bind.unwrap();

        let (mut _top, _) = start_topology(config, false).await;
        wait_for_tcp_in_secs(addr, 30).await;

        let url = format!("http://{}:{}/health", addr.ip(), addr.port());
        let res = reqwest::get(url.as_str())
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        assert!(res.contains("ok"));
    }
}
