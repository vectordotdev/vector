use std::{convert::Infallible, future::ready, time::Duration};

use bytes::Bytes;
use http::Response;
use http_body_util::Full;
use indoc::indoc;
use tokio::time::timeout;

use super::HttpConfig;
use crate::{
    config::{ConfigBuilder, Format, ProviderConfig as _, ProxyConfig},
    signal::{SignalHandler, SignalTo},
    test_util::http::spawn_blackhole_http_server,
};

const CONFIG: &str = indoc! {r#"
    sources:
      http:
        type: http_server
        address: "127.0.0.1:18080"
        auth:
          strategy: bearer
          token: "$${TOKEN}"
          username: "UNUSED_SECRET"
    sinks:
      out:
        type: http
        inputs: [http]
        uri: "http://127.0.0.1:18081/"
        encoding:
          codec: json
        request:
          retry_atempts: 3
"#};

#[tokio::test]
async fn initial_and_polled_configs_retain_ignored_field_warnings() {
    let uri = spawn_blackhole_http_server(|_| {
        ready(Ok::<_, Infallible>(Response::new(Full::new(Bytes::from(
            CONFIG,
        )))))
    })
    .await;

    for interpolate_env in [false, true] {
        let mut provider = HttpConfig {
            url: Some(uri.to_string().parse().unwrap()),
            poll_interval_secs: 1,
            config_format: Format::Yaml,
            interpolate_env,
            proxy: ProxyConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let (mut handler, mut signals) = SignalHandler::new();
        let initial = provider.build(&mut handler).await.unwrap();
        let update = timeout(Duration::from_secs(5), signals.recv())
            .await
            .unwrap()
            .unwrap();
        handler.clear();
        let SignalTo::ReloadFromConfigBuilder(polled) = update else {
            panic!("expected a provider configuration update");
        };
        let expected_input = if interpolate_env {
            CONFIG.replace("$$", "$")
        } else {
            CONFIG.to_owned()
        };
        let expected: ConfigBuilder =
            crate::config::load(expected_input.as_bytes(), Format::Yaml).unwrap();
        let expected = serde_json::to_value(expected).unwrap();
        for builder in [initial, polled] {
            assert_eq!(serde_json::to_value(&builder).unwrap(), expected);
            let (_, warnings) = builder.build_with_warnings().unwrap();
            assert_eq!(warnings.len(), 2);
            assert!(warnings[0].contains("/sinks/out/request/retry_atempts"));
            assert!(warnings[1].contains("/sources/http/auth/username"));
            assert!(!warnings.join(" ").contains("UNUSED_SECRET"));
        }
    }
}
