use std::{fs::File, io::Write, net::SocketAddr, time::Duration};

use futures::Stream;
use tokio::time::sleep;
use vector_lib::event::{Event, EventStatus};
use vrl::event_path;

use crate::{
    SourceSender,
    config::{SourceConfig, SourceContext},
    docker::Container,
    sources::fluent::{FluentConfig, FluentMode, FluentTcpConfig},
    test_util::{
        addr::{PortGuard, next_addr, next_addr_for_ip},
        collect_ready,
        components::{SOCKET_PUSH_SOURCE_TAGS, assert_source_compliance},
        random_string, wait_for_tcp,
    },
};

const FLUENT_BIT_IMAGE: &str = "fluent/fluent-bit";
const FLUENT_BIT_TAG: &str = "1.7";
const FLUENTD_IMAGE: &str = "fluent/fluentd";
const FLUENTD_TAG: &str = "v1.12";

fn make_file(name: &str, content: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut file = File::create(dir.path().join(name)).unwrap();
    write!(&mut file, "{content}").unwrap();
    dir
}

#[tokio::test]
async fn fluentbit() {
    test_fluentbit(EventStatus::Delivered).await;
}

#[tokio::test]
async fn fluentbit_rejection() {
    test_fluentbit(EventStatus::Rejected).await;
}

async fn test_fluentbit(status: EventStatus) {
    assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async move {
        let (_guard, test_address) = next_addr();
        let (out, source_address, _guard) = source(status).await;

        let dir = make_file(
            "fluent-bit.conf",
            &format!(
                r#"
[SERVICE]
Grace      0
Flush      1
Daemon     off

[INPUT]
Name       http
Host       {listen_host}
Port       {listen_port}

[OUTPUT]
Name          forward
Match         *
Host          host.docker.internal
Port          {send_port}
Require_ack_response true
"#,
                listen_host = test_address.ip(),
                listen_port = test_address.port(),
                send_port = source_address.port(),
            ),
        );

        let msg = random_string(64);
        let body = serde_json::json!({ "message": msg });

        let events = Container::new(FLUENT_BIT_IMAGE, FLUENT_BIT_TAG)
            .bind(dir.path().display(), "/fluent-bit/etc")
            .run(async move {
                wait_for_tcp(test_address).await;
                reqwest::Client::new()
                    .post(format!("http://{test_address}/"))
                    .header("content-type", "application/json")
                    .body(body.to_string())
                    .send()
                    .await
                    .unwrap();
                sleep(Duration::from_secs(2)).await;

                collect_ready(out).await
            })
            .await;

        assert_eq!(events.len(), 1);
        let log = events[0].as_log();
        assert_eq!(log["tag"], "http.0".into());
        assert_eq!(log["message"], msg.into());
        assert!(log.get(event_path!("timestamp")).is_some());
        assert!(log.get(event_path!("host")).is_some());
    })
    .await;
}

#[tokio::test]
async fn fluentd() {
    test_fluentd(EventStatus::Delivered, "").await;
}

#[tokio::test]
async fn fluentd_gzip() {
    test_fluentd(EventStatus::Delivered, "compress gzip").await;
}

#[tokio::test]
async fn fluentd_rejection() {
    test_fluentd(EventStatus::Rejected, "").await;
}

async fn test_fluentd(status: EventStatus, options: &str) {
    assert_source_compliance(&SOCKET_PUSH_SOURCE_TAGS, async move {
        let (_guard, test_address) = next_addr();
        let (out, source_address, _guard) = source(status).await;

        let config = format!(
            r#"
<source>
  @type http
  bind {http_host}
  port {http_port}
</source>

<match *>
  @type forward
  <server>
name  local
host  host.docker.internal
port  {port}
  </server>
  <buffer>
flush_mode immediate
  </buffer>
  require_ack_response true
  ack_response_timeout 1
  {options}
</match>
"#,
            http_host = test_address.ip(),
            http_port = test_address.port(),
            port = source_address.port(),
            options = options
        );

        let dir = make_file("fluent.conf", &config);

        let msg = random_string(64);
        let body = serde_json::json!({ "message": msg });

        let events = Container::new(FLUENTD_IMAGE, FLUENTD_TAG)
            .bind(dir.path().display(), "/fluentd/etc")
            .run(async move {
                wait_for_tcp(test_address).await;
                reqwest::Client::new()
                    .post(format!("http://{test_address}/"))
                    .header("content-type", "application/json")
                    .body(body.to_string())
                    .send()
                    .await
                    .unwrap();
                sleep(Duration::from_secs(2)).await;
                collect_ready(out).await
            })
            .await;

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].as_log()["tag"], "".into());
        assert_eq!(events[0].as_log()["message"], msg.into());
        assert!(events[0].as_log().get(event_path!("timestamp")).is_some());
        assert!(events[0].as_log().get(event_path!("host")).is_some());
    })
    .await;
}

async fn source(status: EventStatus) -> (impl Stream<Item = Event> + Unpin, SocketAddr, PortGuard) {
    let (sender, recv) = SourceSender::new_test_finalize(status);
    let (_guard, address) = next_addr_for_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    tokio::spawn(async move {
        FluentConfig {
            mode: FluentMode::Tcp(FluentTcpConfig {
                address: address.into(),
                tls: None,
                keepalive: None,
                permit_origin: None,
                receive_buffer_bytes: None,
                tls_handshake_timeout_secs: None,
                acknowledgements: false.into(),
                connection_limit: None,
            }),
            log_namespace: None,
        }
        .build(SourceContext::new_test(sender, None))
        .await
        .unwrap()
        .await
        .unwrap()
    });
    wait_for_tcp(address).await;
    (recv, address, _guard)
}
