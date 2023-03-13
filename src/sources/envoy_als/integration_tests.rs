use super::{EnvoyAlsConfig, GrpcConfig};
use crate::{
    config::{SourceConfig, SourceContext},
    event::EventStatus,
    test_util::{
        collect_n,
        components::{assert_source_compliance, SOURCE_TAGS},
        retry_until, wait_for_tcp,
    },
    SourceSender,
};
use std::time::Duration;

fn envoy_health_url() -> String {
    std::env::var("ENVOY_HEALTH_URL").unwrap_or_else(|_| "http://0.0.0.0:9001".to_owned())
}

fn envoy_url() -> String {
    std::env::var("ENVOY_URL").unwrap_or_else(|_| "http://0.0.0.0:9000".to_owned())
}

fn source_grpc_address() -> String {
    std::env::var("SOURCE_GRPC_ADDRESS").unwrap_or_else(|_| "0.0.0.0:9999".to_owned())
}

#[tokio::test]
async fn receive_logs() {
    assert_source_compliance(&SOURCE_TAGS, async {
        wait_until_ready(format!("{}/healthz", envoy_health_url())).await;

        let config = EnvoyAlsConfig {
            grpc: GrpcConfig {
                address: source_grpc_address().parse().unwrap(),
                tls: Default::default(),
            },
        };

        let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
        let server = config
            .build(SourceContext::new_test(sender, None))
            .await
            .unwrap();
        tokio::spawn(server);
        wait_for_tcp(source_grpc_address()).await;

        let client = reqwest::Client::new();
        let _res = client
            .get(format!("{}/", envoy_url()))
            .send()
            .await
            .expect("Failed request to Envoy.");

        let output = collect_n(recv, 1).await;
        assert_eq!(output.len(), 1);
    })
    .await;
}

async fn wait_until_ready(address: String) {
    retry_until(
        || async {
            reqwest::get(address.clone())
                .await
                .map_err(|err| err.to_string())
                .and_then(|res| {
                    if res.status().is_success() {
                        Ok(())
                    } else {
                        Err("Not ready yet...".into())
                    }
                })
        },
        Duration::from_secs(1),
        Duration::from_secs(30),
    )
    .await;
}
