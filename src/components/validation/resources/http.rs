use std::{
    collections::VecDeque,
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use axum::{
    response::IntoResponse,
    routing::{MethodFilter, MethodRouter},
    Router,
};
use bytes::BytesMut;
use codecs::{
    encoding, JsonSerializer, LengthDelimitedEncoder, LogfmtSerializer, MetricTagValues,
    NewlineDelimitedEncoder,
};
use http::{Method, Request, StatusCode, Uri};
use hyper::{Body, Client, Server};
use std::future::Future;
use tokio::{
    select,
    sync::{mpsc, oneshot, Mutex, Notify},
};
use tokio_util::codec::{Decoder, Encoder as _};
use vector_core::event::Event;

use crate::{
    codecs::Encoder,
    components::validation::sync::{Configuring, TaskCoordinator},
};

use super::{ResourceCodec, ResourceDirection, TestEvent};

/// An HTTP resource.
#[derive(Clone)]
pub struct HttpResourceConfig {
    uri: Uri,
    method: Option<Method>,
}

impl HttpResourceConfig {
    pub const fn from_parts(uri: Uri, method: Option<Method>) -> Self {
        Self { uri, method }
    }

    pub fn spawn_as_input(
        self,
        direction: ResourceDirection,
        codec: ResourceCodec,
        input_rx: mpsc::Receiver<TestEvent>,
        task_coordinator: &TaskCoordinator<Configuring>,
    ) {
        match direction {
            // The source will pull data from us.
            ResourceDirection::Pull => {
                spawn_input_http_server(self, codec, input_rx, task_coordinator)
            }
            // We'll push data to the source.
            ResourceDirection::Push => {
                spawn_input_http_client(self, codec, input_rx, task_coordinator)
            }
        }
    }

    pub fn spawn_as_output(
        self,
        direction: ResourceDirection,
        codec: ResourceCodec,
        output_tx: mpsc::Sender<Event>,
        task_coordinator: &TaskCoordinator<Configuring>,
    ) {
        match direction {
            // We'll pull data from the sink.
            ResourceDirection::Pull => {
                spawn_output_http_client(self, codec, output_tx, task_coordinator)
            }
            // The sink will push data to us.
            ResourceDirection::Push => {
                spawn_output_http_server(self, codec, output_tx, task_coordinator)
            }
        }
    }
}

/// Spawns an HTTP server that a source will make requests to in order to get events.
#[allow(clippy::missing_const_for_fn)]
fn spawn_input_http_server(
    config: HttpResourceConfig,
    codec: ResourceCodec,
    mut input_rx: mpsc::Receiver<TestEvent>,
    task_coordinator: &TaskCoordinator<Configuring>,
) {
    // This HTTP server will poll the input receiver for input events and buffer them. When a
    // request comes in on the right path/method, one buffered input event will be sent back. If no
    // buffered events are available when the request arrives, an empty response (204 No Content) is
    // returned to the caller.
    let outstanding_events = Arc::new(Mutex::new(VecDeque::new()));

    // First, we'll build and spawn our HTTP server.
    let encoder = codec.into_encoder();
    let sendable_events = Arc::clone(&outstanding_events);

    let (resource_notifier, http_server_shutdown_tx) =
        spawn_http_server(task_coordinator, &config, move |_| {
            let sendable_events = Arc::clone(&sendable_events);
            let mut encoder = encoder.clone();

            async move {
                let mut sendable_events = sendable_events.lock().await;
                if let Some(event) = sendable_events.pop_front() {
                    let mut buffer = BytesMut::new();
                    encode_test_event(&mut encoder, &mut buffer, event);

                    buffer.into_response()
                } else {
                    // No outstanding events to send, so just provide an empty response.
                    StatusCode::NO_CONTENT.into_response()
                }
            }
        });

    // Now we'll create and spawn the resource's core logic loop which drives the buffering of input
    // events and working with the HTTP server as they're consumed.
    let resource_started = task_coordinator.track_started();
    let resource_completed = task_coordinator.track_completed();
    tokio::spawn(async move {
        resource_started.mark_as_done();
        debug!("HTTP server external input resource started.");

        let mut input_finished = false;

        loop {
            select! {
                // Handle input events being sent to us from the runner.
                //
                // When the channel closes, we'll mark the input as being finished so that we know
                // to close the external resource itself once the HTTP server has consumed/sent all
                // outstanding events.
                maybe_event = input_rx.recv(), if !input_finished => match maybe_event {
                    Some(event) => {
                        let mut outstanding_events = outstanding_events.lock().await;
                        outstanding_events.push_back(event);
                    },
                    None => input_finished = true,
                },

                _ = resource_notifier.notified() => {
                    // The HTTP server notified us that it made progress with a send, which is
                    // specifically that it serviced a request which returned a non-zero number of
                    // events.
                    //
                    // This indicates that we need to check and see if our input is completed --
                    // channel closed, no outstanding events left -- and thus if it's time to close.
                    if input_finished {
                        let outstanding_events = outstanding_events.lock().await;
                        if outstanding_events.is_empty() {
                            break
                        }
                    }
                },
            }
        }

        // Mark ourselves as completed now that we've sent all inputs to the source, and
        // additionally signal the HTTP server to also gracefully shutdown.
        _ = http_server_shutdown_tx.send(());
        resource_completed.mark_as_done();

        debug!("HTTP server external input resource completed.");
    });
}

/// Spawns an HTTP client that pushes events to a source which is accepting events over HTTP.
fn spawn_input_http_client(
    config: HttpResourceConfig,
    codec: ResourceCodec,
    mut input_rx: mpsc::Receiver<TestEvent>,
    task_coordinator: &TaskCoordinator<Configuring>,
) {
    // Spin up an HTTP client that will push the input data to the source on a
    // request-per-input-item basis. This runs serially and has no parallelism.
    let started = task_coordinator.track_started();
    let completed = task_coordinator.track_completed();
    let mut encoder = codec.into_encoder();

    tokio::spawn(async move {
        // Mark ourselves as started. We don't actually do anything until we get our first input
        // message, though.
        started.mark_as_done();
        debug!("HTTP client external input resource started.");

        let client = Client::builder().build_http::<Body>();
        let request_uri = config.uri;
        let request_method = config.method.unwrap_or(Method::POST);

        while let Some(event) = input_rx.recv().await {
            debug!("Got event to send from runner.");

            let mut buffer = BytesMut::new();
            encode_test_event(&mut encoder, &mut buffer, event);

            let request = Request::builder()
                .uri(request_uri.clone())
                .method(request_method.clone())
                .body(buffer.freeze().into())
                .expect("should not fail to build request");

            match client.request(request).await {
                Ok(_response) => {
                    // TODO: Emit metric that tracks a successful response from the HTTP server.
                    debug!("Got response from server.");
                }
                Err(e) => {
                    // TODO: Emit metric that tracks a failed response from the HTTP server.
                    error!("Failed to send request: {}", e);
                }
            }
        }

        // Mark ourselves as completed now that we've sent all inputs to the source.
        completed.mark_as_done();

        debug!("HTTP client external input resource completed.");
    });
}

/// Spawns an HTTP server that accepts events sent by a sink.
#[allow(clippy::missing_const_for_fn)]
fn spawn_output_http_server(
    config: HttpResourceConfig,
    codec: ResourceCodec,
    output_tx: mpsc::Sender<Event>,
    task_coordinator: &TaskCoordinator<Configuring>,
) {
    // This HTTP server will wait for events to be sent by a sink, and collect them and send them on
    // via an output sender. We accept/collect events until we're told to shutdown.

    // First, we'll build and spawn our HTTP server.
    let decoder = codec.into_decoder();

    let (_, http_server_shutdown_tx) =
        spawn_http_server(task_coordinator, &config, move |request| {
            let output_tx = output_tx.clone();
            let mut decoder = decoder.clone();

            async move {
                match hyper::body::to_bytes(request.into_body()).await {
                    Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                    Ok(body) => {
                        let mut body = BytesMut::from(&body[..]);
                        loop {
                            match decoder.decode_eof(&mut body) {
                                Ok(Some((events, _byte_size))) => {
                                    for event in events {
                                        output_tx
                                            .send(event)
                                            .await
                                            .expect("should not fail to send output event");
                                    }
                                }
                                Ok(None) => return StatusCode::OK.into_response(),
                                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                            }
                        }
                    }
                }
            }
        });

    // Now we'll create and spawn the resource's core logic loop which simply waits for the runner
    // to instruct us to shutdown, and when that happens, cascades to shutting down the HTTP server.
    let resource_started = task_coordinator.track_started();
    let resource_completed = task_coordinator.track_completed();
    let mut resource_shutdown_rx = task_coordinator.register_for_shutdown();
    tokio::spawn(async move {
        resource_started.mark_as_done();
        debug!("HTTP server external output resource started.");

        resource_shutdown_rx.wait().await;
        _ = http_server_shutdown_tx.send(());
        resource_completed.mark_as_done();

        debug!("HTTP server external output resource completed.");
    });
}

/// Spawns an HTTP client that pulls events by making requests to an HTTP server driven by a sink.
#[allow(clippy::missing_const_for_fn)]
fn spawn_output_http_client(
    _config: HttpResourceConfig,
    _codec: ResourceCodec,
    _output_tx: mpsc::Sender<Event>,
    _task_coordinator: &TaskCoordinator<Configuring>,
) {
    // TODO: The `prometheus_exporter` sink is the only sink that exposes an HTTP server which must be
    // scraped... but since we need special logic to aggregate/deduplicate scraped metrics, we can't
    // use this generically for that purpose.
    todo!()
}

fn spawn_http_server<H, F, R>(
    task_coordinator: &TaskCoordinator<Configuring>,
    config: &HttpResourceConfig,
    handler: H,
) -> (Arc<Notify>, oneshot::Sender<()>)
where
    H: Fn(Request<Body>) -> F + Clone + Send + 'static,
    F: Future<Output = R> + Send,
    R: IntoResponse,
{
    let http_server_started = task_coordinator.track_started();
    let http_server_completed = task_coordinator.track_completed();

    let listen_addr = socketaddr_from_uri(&config.uri);
    let request_path = config
        .uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| "/".to_string());
    let request_method = config.method.clone().unwrap_or(Method::POST);

    // Create our synchronization primitives that are shared between the HTTP server and the
    // resource's core logic loop.
    //
    // This will let the resource be able to trigger the HTTP server to gracefully shutdown, as well
    // as be notified when the HTTP server has served a request, so that it can check if all input
    // events have been sent yet.
    let (http_server_shutdown_tx, http_server_shutdown_rx) = oneshot::channel();
    let resource_notifier = Arc::new(Notify::new());
    let server_notifier = Arc::clone(&resource_notifier);

    tokio::spawn(async move {
        // Create our HTTP server by binding as early as possible to return an error if we can't
        // actually bind.
        let server_builder =
            Server::try_bind(&listen_addr).expect("Failed to bind to listen address.");

        // Create our router, which is a bit boilerplate-y because we take the HTTP method
        // parametrically. We generate a handler that calls the given `handler` and then triggers
        // the notifier shared by the HTTP server and the resource's core logic loop.
        //
        // Every time a request is processed, we notify the core logic loop so it can continue
        // checking to see if it's time to fully close once all input events have been consumed and
        // the input receiver is closed.
        let method_filter = MethodFilter::try_from(request_method)
            .expect("should not fail to convert method to method filter");
        let method_router = MethodRouter::new()
            .fallback(|req: Request<Body>| async move {
                error!(
                    path = req.uri().path(),
                    method = req.method().as_str(),
                    "Component sent request to a different path/method than expected."
                );

                StatusCode::METHOD_NOT_ALLOWED
            })
            .on(method_filter, move |request: Request<Body>| {
                let request_handler = handler(request);
                let notifier = Arc::clone(&server_notifier);

                async move {
                    let response = request_handler.await;
                    notifier.notify_one();
                    response
                }
            });

        let router = Router::new().route(&request_path, method_router);

        // Now actually run/drive the HTTP server and process requests until we're told to shutdown.
        http_server_started.mark_as_done();

        let server = server_builder
            .serve(router.into_make_service())
            .with_graceful_shutdown(async {
                http_server_shutdown_rx.await.ok();
            });

        if let Err(e) = server.await {
            error!(error = ?e, "HTTP server encountered an error.");
        }

        http_server_completed.mark_as_done();
    });

    (resource_notifier, http_server_shutdown_tx)
}

fn socketaddr_from_uri(uri: &Uri) -> SocketAddr {
    let uri_port = uri.port_u16().unwrap_or(80);
    let uri_host = uri
        .host()
        .ok_or_else(|| "host must be present in URI".to_string())
        .and_then(|host| {
            IpAddr::from_str(host)
                .map_err(|_| "URI host must be valid IPv4/IPv6 address".to_string())
        })
        .expect("HTTP URI not valid");

    SocketAddr::from((uri_host, uri_port))
}

pub fn encode_test_event(
    encoder: &mut Encoder<encoding::Framer>,
    buf: &mut BytesMut,
    event: TestEvent,
) {
    match event {
        TestEvent::Passthrough(event) => {
            // Encode the event normally.
            encoder
                .encode(event.into_event(), buf)
                .expect("should not fail to encode input event");
        }
        TestEvent::Modified { event, .. } => {
            // This is a little fragile, but we check what serializer this encoder uses, and based
            // on `Serializer::supports_json`, we choose an opposing codec. For example, if the
            // encoder supports JSON, we'll use a serializer that doesn't support JSON, and vise
            // versa.
            let mut alt_encoder = if encoder.serializer().supports_json() {
                Encoder::<encoding::Framer>::new(
                    LengthDelimitedEncoder::new().into(),
                    LogfmtSerializer::new().into(),
                )
            } else {
                Encoder::<encoding::Framer>::new(
                    NewlineDelimitedEncoder::new().into(),
                    JsonSerializer::new(MetricTagValues::default()).into(),
                )
            };

            alt_encoder
                .encode(event.into_event(), buf)
                .expect("should not fail to encode input event");
        }
    }
}
