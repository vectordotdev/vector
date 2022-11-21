use http::Uri;
use tokio::sync::mpsc;
use tonic::{
    transport::{Channel, Endpoint},
    Request, Response, Status,
};
use vector_core::event::Event;

use crate::{
    components::validation::sync::{Configuring, TaskCoordinator},
    proto::vector::{
        Client as VectorClient, HealthCheckRequest, HealthCheckResponse, PushEventsRequest,
        PushEventsResponse, Server as VectorServer, Service as VectorService, ServingStatus,
    },
};

#[derive(Clone)]
pub struct EventForwardService {
    tx: mpsc::Sender<Event>,
}

impl From<mpsc::Sender<Event>> for EventForwardService {
    fn from(tx: mpsc::Sender<Event>) -> Self {
        Self { tx }
    }
}

#[tonic::async_trait]
impl VectorService for EventForwardService {
    async fn push_events(
        &self,
        request: Request<PushEventsRequest>,
    ) -> Result<Response<PushEventsResponse>, Status> {
        let events = request.into_inner().events.into_iter().map(Event::from);

        for event in events {
            self.tx
                .send(event)
                .await
                .expect("event forward rx should not close first");
        }

        Ok(Response::new(PushEventsResponse {}))
    }

    async fn health_check(
        &self,
        _: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let message = HealthCheckResponse {
            status: ServingStatus::Serving.into(),
        };

        Ok(Response::new(message))
    }
}

pub struct InputEdge {
    client: VectorClient<Channel>,
}

pub struct OutputEdge {
    listen_address: Uri,
    server: VectorServer<EventForwardService>,
    rx: mpsc::Receiver<Event>,
}

impl InputEdge {
    pub fn from_address(address: Uri) -> Self {
        let channel = Endpoint::from(address).connect_lazy();

        Self {
            client: VectorClient::new(channel),
        }
    }

    pub fn spawn_input_client(
        self,
        task_coordinator: &TaskCoordinator<Configuring>,
    ) -> mpsc::Sender<Event> {
        let (tx, mut rx) = mpsc::channel(1024);
        let started = task_coordinator.track_started();
        let completed = task_coordinator.track_completed();

        tokio::spawn(async move {
            started.mark_as_done();

            // TODO: Read events from `rx` and send them to the component topology via our Vector
            // gRPC client that connects to the Vector source.
            while let Some(_event) = rx.recv().await {}

            completed.mark_as_done();
        });

        tx
    }
}

impl OutputEdge {
    pub fn from_address(listen_address: Uri) -> Self {
        let (tx, rx) = mpsc::channel(1024);

        Self {
            listen_address,
            server: VectorServer::new(EventForwardService::from(tx)),
            rx,
        }
    }

    pub fn spawn_output_server(
        self,
        task_coordinator: &TaskCoordinator<Configuring>,
    ) -> mpsc::Receiver<Event> {
        let started = task_coordinator.track_started();
        let completed = task_coordinator.track_completed();

        tokio::spawn(async move {
            started.mark_as_done();

            // TODO: Spawn the Vector gRPC server, which will listen for events and forward them to
            // `rx`.
            //
            // TODO: We need to thread a shutdown trigger to this task so it knows when to actually
            // shutdown, otherwise we'll be listening forever and never make it to marking ourselves
            // as completed below.

            completed.mark_as_done();
        });

        self.rx
    }
}

pub struct ControlledEdges {
    pub input: Option<mpsc::Sender<Event>>,
    pub output: Option<mpsc::Receiver<Event>>,
}
