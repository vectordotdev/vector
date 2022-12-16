use bytes::BytesMut;
use codecs::encoding;
use http::{Method, Request, Uri};
use hyper::{Body, Client};
use tokio::sync::mpsc;
use tokio_util::codec::Encoder as _;
use vector_core::event::Event;

use crate::{
    codecs::Encoder,
    components::validation::sync::{Configuring, TaskCoordinator},
};

use super::{ResourceCodec, ResourceDirection, TestEvent};

/// An HTTP resource.
pub struct HttpConfig {
    uri: Uri,
    method: Option<Method>,
}

impl HttpConfig {
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
    _config: HttpConfig,
    _codec: ResourceCodec,
    _input_rx: mpsc::Receiver<TestEvent>,
    _task_coordinator: &TaskCoordinator<Configuring>,
) {
    // Spin up an HTTP server that responds with all of the input data it has received since the
    // last request was responded to. Essentially, a client calling the server will never see data
    // more than once.
}

fn spawn_input_http_client(
    config: HttpConfig,
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
    _config: HttpConfig,
    _codec: ResourceCodec,
    _output_tx: mpsc::Sender<Event>,
    _task_coordinator: &TaskCoordinator<Configuring>,
) {
}

#[allow(clippy::missing_const_for_fn)]
fn spawn_output_http_client(
    _config: HttpConfig,
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
            // TODO: Actually use a different encoder to encode this.
            encoder
                .encode(event.into_event(), buf)
                .expect("should not fail to encode input event");
        }
    }
}
