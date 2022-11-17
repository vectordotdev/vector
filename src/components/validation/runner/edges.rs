use http::Uri;
use tokio::sync::mpsc;
use tonic::{transport::{Channel, Endpoint}, Request, Response, Status};
use vector_core::event::Event;

use crate::proto::vector::{
    Client as VectorClient, HealthCheckRequest, HealthCheckResponse, PushEventsRequest,
    PushEventsResponse, Server as VectorServer, Service as VectorService, ServingStatus,
};

// TODO: call this "basic vector service" or something since we'll also need it for collecting the
// telemetry data from the running topology
#[derive(Clone)]
pub struct EventForwardService {
    tx: mpsc::Sender<Vec<Event>>
}

#[tonic::async_trait]
impl VectorService for EventForwardService {
    async fn push_events(
        &self,
        request: Request<PushEventsRequest>,
    ) -> Result<Response<PushEventsResponse>, Status> {
        let events = request.into_inner()
            .events
            .into_iter()
            .map(Event::from)
            .collect();

        self.tx.send(events).await.expect("event forward rx should not close first");

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
    rx: mpsc::Receiver<Vec<Event>>,
}

pub enum ControlledEdge {
    Input(InputEdge),
    Output(OutputEdge),
}

impl ControlledEdge {
    pub fn input(address: Uri) -> Self {
        let channel = Endpoint::from(address).connect_lazy();

        Self::Input(InputEdge {
            client: VectorClient::new(channel),
        })
    }

    pub fn output(listen_address: Uri) -> Self {
        let (tx, rx) = mpsc::channel(1024);

        Self::Output(OutputEdge {
            listen_address,
            server: VectorServer::new(EventForwardService { tx }),
            rx,
        })
    }
}
