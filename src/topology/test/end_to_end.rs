use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server, StatusCode,
};
use tokio::{
    sync::{mpsc, oneshot, Mutex},
    task::JoinHandle,
    time::{timeout, Duration},
};

use super::{RunningTopology, TopologyPieces};
use crate::{
    config::{self, ConfigDiff, Format},
    test_util, Error,
};

type Lock = Arc<Mutex<()>>;

pub async fn respond(
    waiter: Lock,
    tx: mpsc::Sender<()>,
    status: StatusCode,
) -> Result<Response<Body>, Error> {
    tx.send(())
        .await
        .expect("Error sending 'before' status from test server");
    _ = waiter.lock().await;
    Ok(Response::builder()
        .status(status)
        .body(Body::empty())
        .unwrap())
}

pub async fn http_server(
    address: SocketAddr,
    waiter: Lock,
    status: StatusCode,
) -> mpsc::Receiver<()> {
    let (tx, rx) = mpsc::channel(1);
    let service = make_service_fn(move |_| {
        let waiter = Arc::clone(&waiter);
        let tx = tx.clone();
        async move {
            Ok::<_, Error>(service_fn(move |_req: Request<Body>| {
                respond(Arc::clone(&waiter), tx.clone(), status)
            }))
        }
    });

    let server = Server::bind(&address).serve(service);
    tokio::spawn(server);

    test_util::wait_for_tcp(address).await;

    rx
}

pub fn http_client(
    address: SocketAddr,
    body: impl Into<String>,
) -> (oneshot::Receiver<()>, JoinHandle<reqwest::Response>) {
    let body = body.into();
    let (tx, rx) = oneshot::channel();

    let sender = tokio::spawn(async move {
        let result = reqwest::Client::new()
            .post(&format!("http://{}/", address))
            .body(body)
            .send()
            .await
            .expect("Error receiving response from HTTP sink");
        tx.send(()).unwrap();
        result
    });
    (rx, sender)
}

async fn http_to_http(status: StatusCode, response: StatusCode) {
    test_util::trace_init();

    let address1 = test_util::next_addr();
    let address2 = test_util::next_addr();
    let config = config::load_from_str(
        &format!(
            r#"
[sources.in]
type = "http"
address = "{address1}"
acknowledgements.enabled = true

[sinks.out]
type = "http"
inputs = ["in"]
encoding.codec = "json"
uri = "http://{address2}/"
"#,
            address1 = address1,
            address2 = address2,
        ),
        Format::Toml,
    )
    .unwrap();
    let diff = ConfigDiff::initial(&config);
    let pieces =
        TopologyPieces::build_or_log_errors(&config, &diff, HashMap::new(), Default::default())
            .await
            .unwrap();
    let (_topology, _) = RunningTopology::start_validated(config, diff, pieces)
        .await
        .unwrap();

    test_util::wait_for_tcp(address1).await;

    let mutex = Arc::new(Mutex::new(()));
    let pause = mutex.lock().await;
    let mut rx_server = http_server(address2, Arc::clone(&mutex), status).await;

    // The expected flow is this:
    // 0. Nothing is ready to continue.
    assert!(rx_server.try_recv().is_err());

    // 1. We send an event to the HTTP source server.
    let (mut rx_client, sender) = http_client(address1, "test");

    // 2. It sends to the HTTP sink sender. `rx` is activated, but no response yet.
    timeout(Duration::from_secs(4), rx_server.recv())
        .await
        .expect("Timed out waiting to receive event from HTTP sink")
        .expect("Error receiving event from HTTP sink");
    assert!(rx_client.try_recv().is_err());

    // 3. Our test HTTP server waits for the mutex lock.
    drop(pause);
    assert!(rx_server.try_recv().is_err());

    // 4. Our test HTTP server responds.
    // 5. The acknowledgement is returned to the source.
    // 6. The source responds to our initial send.
    let result = timeout(Duration::from_secs(1), sender)
        .await
        .expect("Timed out waiting to receive result from HTTP source")
        .expect("Error receiving result from tokio task");
    assert_eq!(result.status(), response);
}

#[tokio::test]
async fn http_to_http_delivered() {
    http_to_http(StatusCode::OK, StatusCode::OK).await;
}

#[tokio::test]
async fn http_to_http_failed() {
    http_to_http(StatusCode::FORBIDDEN, StatusCode::BAD_REQUEST).await;
}
