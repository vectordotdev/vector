use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use axum::{
    response::IntoResponse,
    routing::{MethodFilter, MethodRouter},
    Router,
};
use bytes::{BufMut as _, BytesMut};
use http::{Method, Request, StatusCode, Uri};
use hyper::{Body, Client, Server};
use tokio::{
    select,
    sync::{mpsc, oneshot, Mutex, Notify},
};
use tokio_util::codec::Decoder;

use crate::components::validation::{
    sync::{Configuring, TaskCoordinator},
    RunnerMetrics,
};
use vector_lib::{
    codecs::encoding::Framer, codecs::encoding::Serializer::Json,
    codecs::CharacterDelimitedEncoder, config::LogNamespace, event::Event,
    EstimatedJsonEncodedSizeOf,
};

use super::{encode_test_event, ResourceCodec, ResourceDirection, TestEvent};

/// An HTTP resource.
#[derive(Clone)]
pub struct HttpResourceConfig {
    uri: Uri,
    method: Option<Method>,
    headers: Option<HashMap<String, String>>,
}

impl HttpResourceConfig {
    pub const fn from_parts(uri: Uri, method: Option<Method>) -> Self {
        Self {
            uri,
            method,
            headers: None,
        }
    }

    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = Some(headers);
        self
    }

    pub fn spawn_as_input(
        self,
        direction: ResourceDirection,
        codec: ResourceCodec,
        input_rx: mpsc::Receiver<TestEvent>,
        task_coordinator: &TaskCoordinator<Configuring>,
        runner_metrics: &Arc<Mutex<RunnerMetrics>>,
    ) {
        match direction {
            // The source will pull data from us.
            ResourceDirection::Pull => {
                spawn_input_http_server(self, codec, input_rx, task_coordinator, runner_metrics)
            }
            // We'll push data to the source.
            ResourceDirection::Push => {
                spawn_input_http_client(self, codec, input_rx, task_coordinator, runner_metrics)
            }
        }
    }

    pub fn spawn_as_output(self, ctx: HttpResourceOutputContext) -> vector_lib::Result<()> {
        match ctx.direction {
            // We'll pull data from the sink.
            ResourceDirection::Pull => Ok(ctx.spawn_output_http_client(self)),
            // The sink will push data to us.
            ResourceDirection::Push => ctx.spawn_output_http_server(self),
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
    runner_metrics: &Arc<Mutex<RunnerMetrics>>,
) {
    // This HTTP server will poll the input receiver for input events and buffer them. When a
    // request comes in on the right path/method, one buffered input event will be sent back. If no
    // buffered events are available when the request arrives, an empty response (204 No Content) is
    // returned to the caller.
    let outstanding_events = Arc::new(Mutex::new(VecDeque::new()));

    // First, we'll build and spawn our HTTP server.
    let encoder = codec.into_encoder();
    let sendable_events = Arc::clone(&outstanding_events);

    let (resource_notifier, http_server_shutdown_tx) = spawn_http_server(
        task_coordinator,
        &config,
        runner_metrics,
        move |_request, _runner_metrics| {
            let sendable_events = Arc::clone(&sendable_events);
            let mut encoder = encoder.clone();

            async move {
                let mut sendable_events = sendable_events.lock().await;
                if let Some(event) = sendable_events.pop_front() {
                    let mut buffer = BytesMut::new();
                    encode_test_event(&mut encoder, &mut buffer, event);

                    buffer.into_response()
                } else {
                    // We'll send an empty 200 in the response since some
                    // sources throw errors for anything other than a valid
                    // response.
                    StatusCode::OK.into_response()
                }
            }
        },
    );

    // Now we'll create and spawn the resource's core logic loop which drives the buffering of input
    // events and working with the HTTP server as they're consumed.
    let resource_started = task_coordinator.track_started();
    let resource_completed = task_coordinator.track_completed();
    let mut resource_shutdown_rx = task_coordinator.register_for_shutdown();

    tokio::spawn(async move {
        resource_started.mark_as_done();
        info!("HTTP server external input resource started.");

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
                    None => {
                        info!("HTTP server external input resource input is finished.");
                        input_finished = true;
                    },
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
        info!("HTTP server external input resource signalling ready for shutdown.");

        // Wait for the runner to signal us to shutdown
        resource_shutdown_rx.wait().await;

        // Shutdown the server
        _ = http_server_shutdown_tx.send(());

        info!("HTTP server external input resource marking as done.");
        resource_completed.mark_as_done();

        info!("HTTP server external input resource completed.");
    });
}

/// Spawns an HTTP client that pushes events to a source which is accepting events over HTTP.
fn spawn_input_http_client(
    config: HttpResourceConfig,
    codec: ResourceCodec,
    mut input_rx: mpsc::Receiver<TestEvent>,
    task_coordinator: &TaskCoordinator<Configuring>,
    runner_metrics: &Arc<Mutex<RunnerMetrics>>,
) {
    // Spin up an HTTP client that will push the input data to the source on a
    // request-per-input-item basis. This runs serially and has no parallelism.
    let started = task_coordinator.track_started();
    let completed = task_coordinator.track_completed();
    let mut encoder = codec.into_encoder();
    let runner_metrics = Arc::clone(runner_metrics);

    tokio::spawn(async move {
        // Mark ourselves as started. We don't actually do anything until we get our first input
        // message, though.
        started.mark_as_done();
        info!("HTTP client external input resource started.");

        let client = Client::builder().build_http::<Body>();
        let request_uri = config.uri;
        let request_method = config.method.unwrap_or(Method::POST);
        let headers = config.headers.unwrap_or_default();

        while let Some(event) = input_rx.recv().await {
            debug!("Got event to send from runner.");

            let mut buffer = BytesMut::new();

            let is_json = matches!(encoder.serializer(), Json(_))
                && matches!(
                    encoder.framer(),
                    Framer::CharacterDelimited(CharacterDelimitedEncoder { delimiter: b',' })
                );

            if is_json {
                buffer.put_u8(b'[');
            }

            encode_test_event(&mut encoder, &mut buffer, event);

            if is_json {
                if !buffer.is_empty() {
                    // remove trailing comma from last record
                    buffer.truncate(buffer.len() - 1);
                }
                buffer.put_u8(b']');

                // in this edge case we have removed the trailing comma (one byte) and added
                // opening and closing braces (2 bytes) for a net add of one byte.
                let mut runner_metrics = runner_metrics.lock().await;
                runner_metrics.sent_bytes_total += 1;
            }

            let mut request_builder = Request::builder()
                .uri(request_uri.clone())
                .method(request_method.clone());

            for (key, value) in &headers {
                request_builder = request_builder.header(key, value);
            }

            let request = request_builder
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

        info!("HTTP client external input resource completed.");
    });
}

/// Anything that the output side HTTP external resource needs
pub struct HttpResourceOutputContext<'a> {
    pub direction: ResourceDirection,
    pub codec: ResourceCodec,
    pub output_tx: mpsc::Sender<Vec<Event>>,
    pub task_coordinator: &'a TaskCoordinator<Configuring>,
    pub input_events: Vec<TestEvent>,
    pub runner_metrics: &'a Arc<Mutex<RunnerMetrics>>,
    pub log_namespace: LogNamespace,
}

impl HttpResourceOutputContext<'_> {
    /// Spawns an HTTP server that accepts events sent by a sink.
    #[allow(clippy::missing_const_for_fn)]
    fn spawn_output_http_server(&self, config: HttpResourceConfig) -> vector_lib::Result<()> {
        // This HTTP server will wait for events to be sent by a sink, and collect them and send them on
        // via an output sender. We accept/collect events until we're told to shutdown.

        // First, we'll build and spawn our HTTP server.
        let decoder = self.codec.into_decoder(self.log_namespace)?;

        // Note that we currently don't differentiate which events should and shouldn't be rejected-
        // we reject all events in this server if any are marked for rejection.
        // In the future it might be useful to be able to select which to reject. That will involve
        // adding logic to the test case which is passed down here, and to the event itself. Since
        // we can't guarantee the order of events, we'd need a way to flag which ones need to be
        // rejected.
        let should_reject = self
            .input_events
            .iter()
            .filter(|te| te.should_reject())
            .count()
            > 0;

        let output_tx = self.output_tx.clone();
        let (_, http_server_shutdown_tx) = spawn_http_server(
            self.task_coordinator,
            &config,
            self.runner_metrics,
            move |request, output_runner_metrics| {
                let output_tx = output_tx.clone();
                let mut decoder = decoder.clone();

                async move {
                    match hyper::body::to_bytes(request.into_body()).await {
                        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                        Ok(body) => {
                            let mut body = BytesMut::from(&body[..]);
                            loop {
                                match decoder.decode_eof(&mut body) {
                                    Ok(Some((events, byte_size))) => {
                                        if should_reject {
                                            info!("HTTP server external output resource decoded {byte_size} bytes but test case configured to reject.");
                                        } else {
                                            let mut output_runner_metrics =
                                                output_runner_metrics.lock().await;
                                            info!("HTTP server external output resource decoded {byte_size} bytes.");

                                            // Update the runner metrics for the received events. This will later
                                            // be used in the Validators, as the "expected" case.
                                            output_runner_metrics.received_bytes_total +=
                                                byte_size as u64;

                                            output_runner_metrics.received_events_total +=
                                                events.len() as u64;

                                            events.iter().for_each(|event| {
                                                output_runner_metrics.received_event_bytes_total +=
                                                    event.estimated_json_encoded_size_of().get()
                                                        as u64;
                                            });

                                            output_tx
                                                .send(events.to_vec())
                                                .await
                                                .expect("should not fail to send output event");
                                        }
                                    }
                                    Ok(None) => {
                                        if should_reject {
                                            // This status code is not retried and should result in the component under test
                                            // emitting error events
                                            return StatusCode::BAD_REQUEST.into_response();
                                        } else {
                                            return StatusCode::OK.into_response();
                                        }
                                    }
                                    Err(_) => {
                                        error!(
                                            "HTTP server failed to decode {:?}",
                                            String::from_utf8_lossy(&body)
                                        );
                                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                                    }
                                }
                            }
                        }
                    }
                }
            },
        );

        // Now we'll create and spawn the resource's core logic loop which simply waits for the runner
        // to instruct us to shutdown, and when that happens, cascades to shutting down the HTTP server.
        let resource_started = self.task_coordinator.track_started();
        let resource_completed = self.task_coordinator.track_completed();
        let mut resource_shutdown_rx = self.task_coordinator.register_for_shutdown();

        tokio::spawn(async move {
            resource_started.mark_as_done();
            info!("HTTP server external output resource started.");

            // Wait for the runner to tell us to shutdown
            resource_shutdown_rx.wait().await;

            // signal the server to shutdown
            let _ = http_server_shutdown_tx.send(());

            // mark ourselves as done
            resource_completed.mark_as_done();

            info!("HTTP server external output resource completed.");
        });

        Ok(())
    }

    /// Spawns an HTTP client that pulls events by making requests to an HTTP server driven by a sink.
    #[allow(clippy::missing_const_for_fn)]
    fn spawn_output_http_client(&self, _config: HttpResourceConfig) {
        // TODO: The `prometheus_exporter` sink is the only sink that exposes an HTTP server which must be
        // scraped... but since we need special logic to aggregate/deduplicate scraped metrics, we can't
        // use this generically for that purpose.
        todo!()
    }
}

fn spawn_http_server<H, F, R>(
    task_coordinator: &TaskCoordinator<Configuring>,
    config: &HttpResourceConfig,
    runner_metrics: &Arc<Mutex<RunnerMetrics>>,
    handler: H,
) -> (Arc<Notify>, oneshot::Sender<()>)
where
    H: Fn(Request<Body>, Arc<Mutex<RunnerMetrics>>) -> F + Clone + Send + 'static,
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

    let output_runner_metrics = Arc::clone(runner_metrics);

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
                let request_handler = handler(request, output_runner_metrics);
                let notifier = Arc::clone(&server_notifier);

                async move {
                    let response = request_handler.await;
                    notifier.notify_one();
                    response
                }
            });

        let router = Router::new().route(&request_path, method_router).fallback(
            |req: Request<Body>| async move {
                error!(?req, "Component sent request the server could not route.");
                StatusCode::NOT_FOUND
            },
        );

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
