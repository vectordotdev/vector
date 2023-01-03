use std::{
    collections::VecDeque,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    str::FromStr,
    sync::Arc,
};

use bytes::BytesMut;
use codecs::{
    encoding, JsonSerializer, LengthDelimitedEncoder, LogfmtSerializer, NewlineDelimitedEncoder,
};
use http::{uri::PathAndQuery, Method, Request, Response, Uri};
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Client, Error, Server,
};
use tokio::{
    select,
    sync::{mpsc, oneshot, Mutex, Notify},
};
use tokio_util::codec::Encoder as _;
use vector_core::event::Event;

use crate::{
    codecs::Encoder,
    components::validation::sync::{Configuring, TaskCoordinator},
};

use super::{ResourceCodec, ResourceDirection, TestEvent};

/// An HTTP resource.
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
            // The sink will push data data to us.
            ResourceDirection::Push => {
                spawn_output_http_server(self, codec, output_tx, task_coordinator)
            }
        }
    }
}

#[allow(clippy::missing_const_for_fn)]
fn spawn_input_http_server(
    config: HttpResourceConfig,
    codec: ResourceCodec,
    mut input_rx: mpsc::Receiver<TestEvent>,
    task_coordinator: &TaskCoordinator<Configuring>,
) {
    // Spin up an HTTP server that responds with all of the input data it has received since the
    // last request was responded to. Essentially, a client calling the server will never see data
    // more than once.
    let resource_started = task_coordinator.track_started();
    let http_server_started = task_coordinator.track_started();
    let resource_completed = task_coordinator.track_completed();
    let http_server_completed = task_coordinator.track_completed();
    let encoder = codec.into_encoder();

    tokio::spawn(async move {
        let uri_port = config.uri.port_u16().unwrap_or(80);
        let uri_host = config
            .uri
            .host()
            .ok_or_else(|| "host must be present in URI".to_string())
            .and_then(|host| {
                Ipv4Addr::from_str(host)
                    .map_err(|_| "URI host must be valid IPv4 address".to_string())
            })
            .expect("HTTP URI not valid");

        let listen_addr = SocketAddr::from(SocketAddrV4::new(uri_host, uri_port));
        let server_builder = Server::try_bind(&listen_addr)
            .expect("failed to bind HTTP server external input listen address");

        let request_path = config
            .uri
            .path_and_query()
            .cloned()
            .or_else(|| Some(PathAndQuery::from_static("/")));
        let request_method = config.method.unwrap_or(Method::POST);

        let outstanding_events = Arc::new(Mutex::new(VecDeque::new()));
        let server_notifier = Arc::new(Notify::new());

        let sendable_events = Arc::clone(&outstanding_events);
        let resource_notifier = Arc::clone(&server_notifier);

        let make_svc = make_service_fn(move |_| {
            let path = request_path.clone();
            let method = request_method.clone();
            let sendable_events = Arc::clone(&sendable_events);
            let resource_notifier = Arc::clone(&resource_notifier);
            let encoder = encoder.clone();

            async move {
                Ok::<_, Error>(service_fn(move |req| {
                    let path = path.clone();
                    let method = method.clone();
                    let sendable_events = Arc::clone(&sendable_events);
                    let resource_notifier = Arc::clone(&resource_notifier);
                    let mut encoder = encoder.clone();

                    async move {
                        let actual_path = req.uri().path_and_query();
                        let actual_method = req.method();

                        if actual_method == method && actual_path == path.as_ref() {
                            let mut sendable_events = sendable_events.lock().await;
                            if let Some(event) = sendable_events.pop_front() {
                                let mut buffer = BytesMut::new();
                                encode_test_event(&mut encoder, &mut buffer, event);

                                // Gotta notify the resource before we technically send back the response.
                                resource_notifier.notify_one();

                                Ok(Response::new(Body::from(buffer.freeze())))
                            } else {
                                // No outstanding events to send, so just provide an empty response.
                                Ok(Response::new(Body::empty()))
                            }
                        } else {
                            // TODO: We probably need/want to capture a metric for these errors.

                            error!(
                                expected_path = ?path, actual_path = ?actual_path,
                                expected_method = ?method, actual_method = ?actual_method,
                                "Component sent request to a different path/method than expected."
                            );

                            Response::builder().status(400).body(Body::empty())
                        }
                    }
                }))
            }
        });

        // Spawn the HTTP server and start our resource loop.
        let (http_server_shutdown_tx, http_server_shutdown_rx) = oneshot::channel();
        tokio::spawn(async move {
            http_server_started.mark_as_done();

            let server = server_builder
                .serve(make_svc)
                .with_graceful_shutdown(async {
                    http_server_shutdown_rx.await.ok();
                });

            if let Err(e) = server.await {
                error!(error = ?e, "HTTP server encountered an error.");
            }

            http_server_completed.mark_as_done();
        });

        // Now that we've spawned the HTTP server task, we can mark ourselves as started. The HTTP
        // server task will also mark itself as started/completed as well.
        //
        // We could be more precise and use a barrier to only need a single start/complete token
        // pair, but this is slightly cleaner.
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

                _ = server_notifier.notified() => {
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
        let _ = http_server_shutdown_tx.send(());
        resource_completed.mark_as_done();

        debug!("HTTP server external input resource completed.");
    });
}

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

#[allow(clippy::missing_const_for_fn)]
fn spawn_output_http_server(
    _config: HttpResourceConfig,
    _codec: ResourceCodec,
    _output_tx: mpsc::Sender<Event>,
    _task_coordinator: &TaskCoordinator<Configuring>,
) {
}

#[allow(clippy::missing_const_for_fn)]
fn spawn_output_http_client(
    _config: HttpResourceConfig,
    _codec: ResourceCodec,
    _output_tx: mpsc::Sender<Event>,
    _task_coordinator: &TaskCoordinator<Configuring>,
) {
}

fn encode_test_event(
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
                    JsonSerializer::new().into(),
                )
            };

            alt_encoder
                .encode(event.into_event(), buf)
                .expect("should not fail to encode input event");
        }
    }
}
