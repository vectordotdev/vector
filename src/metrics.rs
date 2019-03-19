use futures::{future, Future, Poll, Stream};
use hotmic::{Controller, Receiver, Sink, Snapshot};
use hyper::{Body, Request, Response};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_hyper::body::LiftBody;
use tower_hyper::server::Server;
use tower_service::Service;

/// Create the metrics sink and provide the server Service
pub fn metrics() -> (Sink<String>, MetricsServer) {
    let mut receiver = Receiver::builder().build();
    let controller = receiver.get_controller();
    let sink = receiver.get_sink();

    std::thread::spawn(move || {
        receiver.run();
    });

    let server = MetricsServer { controller };

    (sink, server)
}

/// Represents the Server that serves the metrics
pub struct MetricsServer {
    controller: Controller,
}

pub struct MetricsServerSvc {
    snapshot: Snapshot,
}

/// Start a Tcplistener and serve the metrics server on that socket
pub fn serve(addr: SocketAddr, svc: MetricsServer) -> impl Future<Item = (), Error = ()> {
    let bind = TcpListener::bind(&addr).expect("Unable to bind metrics server address");

    info!("Serving metrics on: {}", addr);

    let server = Server::new(svc);

    bind.incoming()
        .fold(server, |mut server, stream| {
            if let Err(e) = stream.set_nodelay(true) {
                return Err(e);
            }

            tokio::spawn(
                server
                    .serve(stream)
                    .map_err(|e| panic!("Server error {:?}", e)),
            );

            Ok(server)
        })
        .map_err(|e| panic!("metrics serve error: {:?}", e))
        .map(|_| {})
}

impl Service<()> for MetricsServer {
    type Response = MetricsServerSvc;
    type Error = hyper::Error;
    type Future = future::FutureResult<Self::Response, Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, _: ()) -> Self::Future {
        let snapshot = self.controller.get_snapshot().unwrap();
        future::ok(MetricsServerSvc { snapshot })
    }
}

impl Service<Request<Body>> for MetricsServerSvc {
    type Response = Response<LiftBody<Body>>;
    type Error = hyper::Error;
    type Future = future::FutureResult<Self::Response, Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, _req: Request<Body>) -> Self::Future {
        let snapshot = serde_json::to_vec(&self.snapshot).unwrap();
        let body = LiftBody::new(Body::from(snapshot));
        let res = Response::new(body);
        future::ok(res)
    }
}
