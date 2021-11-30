use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use vector::config::{self, ConfigDiff, Format};
use vector::test_util::{next_addr, wait_for_tcp};
use vector::topology;
use vector::Error;

type Lock = Arc<Mutex<()>>;

pub async fn respond(
    waiter: Lock,
    tx: mpsc::Sender<()>,
    status: StatusCode,
) -> Result<Response<Body>, Error> {
    tx.send(())
        .await
        .expect("Error sending 'before' status from test server");
    waiter.lock().await;
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

    wait_for_tcp(address).await;

    rx
}

#[cfg(all(feature = "sources-http", feature = "sinks-http"))]
async fn http_to_http(status: StatusCode, response: StatusCode) {
    let address1 = next_addr();
    let address2 = next_addr();
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
encoding = "json"
uri = "http://{address2}/"
"#,
            address1 = address1,
            address2 = address2,
        ),
        Some(Format::Toml),
    )
    .unwrap();
    let diff = ConfigDiff::initial(&config);
    let pieces = topology::build_or_log_errors(&config, &diff, HashMap::new())
        .await
        .unwrap();
    let (_topology, _shutdown) = topology::start_validated(config, diff, pieces)
        .await
        .unwrap();

    wait_for_tcp(address1).await;

    let mutex = Arc::new(Mutex::new(()));
    let pause = mutex.lock().await;
    let mut rx = http_server(address2, Arc::clone(&mutex), status).await;

    // The expected flow is this:
    // 0. Nothing is ready to continue.
    assert!(matches!(rx.try_recv(), Err(_)));
    // 1. We send an event to the HTTP source server.
    let send = reqwest::Client::new()
        .post(&format!("http://{}/", address1))
        .body("test".to_owned())
        .send();
    let sender = tokio::spawn(send);
    // 2. It sends to the HTTP sink sender. `rx` is sent, but `rx2` has not completed.
    rx.recv()
        .await
        .expect("Error receiving event from HTTP sink");
    // 3. Our test HTTP server waits for the mutex lock.
    drop(pause);
    assert!(matches!(rx.try_recv(), Err(_)));
    // 4. Our test HTTP server responds.
    // 5. The acknowledgement is returned to the source.
    // 6. The source responds to our initial send.
    let result = sender
        .await
        .expect("Error receiving result from tokio task")
        .expect("Error receiving response from HTTP source");
    assert_eq!(result.status(), response);
}

#[cfg(all(feature = "sources-http", feature = "sinks-http"))]
#[tokio::test]
async fn http_to_http_delivered() {
    http_to_http(StatusCode::OK, StatusCode::OK).await;
}

#[cfg(all(feature = "sources-http", feature = "sinks-http"))]
#[tokio::test]
async fn http_to_http_failed() {
    http_to_http(StatusCode::FORBIDDEN, StatusCode::BAD_REQUEST).await;
}
