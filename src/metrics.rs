use futures::{future, Future, Poll, Stream};
use hotmic::{
    snapshot::{Snapshot, TypedMeasurement},
    Controller, Receiver, Sink,
};
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
    snapshot: Option<Snapshot>,
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
        let snapshot = Some(self.controller.get_snapshot().unwrap());
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
        let snapshot = self.snapshot.take().unwrap();
        let snapshot = process_snapshot(snapshot).unwrap();
        let body = LiftBody::new(Body::from(snapshot));
        let res = Response::new(body);
        future::ok(res)
    }
}

// taken from https://github.com/nuclearfurnace/hotmic-prometheus/blob/master/src/lib.rs
fn process_snapshot(snapshot: Snapshot) -> Result<String, ()> {
    let mut output = String::from("# hotmic-prometheus exporter\n");

    for measurement in snapshot.into_vec() {
        output.push_str("\n");

        match measurement {
            TypedMeasurement::Counter(label, value) => {
                let label = label.replace('.', "_");
                output.push_str("# TYPE ");
                output.push_str(label.as_str());
                output.push_str(" counter\n");
                output.push_str(label.as_str());
                output.push_str(" ");
                output.push_str(value.to_string().as_str());
                output.push_str("\n");
            }
            TypedMeasurement::Gauge(label, value) => {
                let label = label.replace('.', "_");
                output.push_str("# TYPE ");
                output.push_str(label.as_str());
                output.push_str(" gauge\n");
                output.push_str(label.as_str());
                output.push_str(" ");
                output.push_str(value.to_string().as_str());
                output.push_str("\n");
            }
            TypedMeasurement::TimingHistogram(label, summary) => {
                let label = label.replace('.', "_");
                output.push_str("# TYPE ");
                output.push_str(label.as_str());
                output.push_str("_nanoseconds summary\n");
                for (percentile, value) in summary.measurements() {
                    output.push_str(label.as_str());
                    output.push_str("_nanoseconds{quantile=\"");
                    output.push_str(percentile.as_quantile().to_string().as_str());
                    output.push_str("\"} ");
                    output.push_str(value.to_string().as_str());
                    output.push_str("\n");
                }
                output.push_str(label.as_str());
                output.push_str("_nanoseconds_sum ");
                output.push_str(summary.sum().to_string().as_str());
                output.push_str("\n");
                output.push_str(label.as_str());
                output.push_str("_nanoseconds_count ");
                output.push_str(summary.count().to_string().as_str());
                output.push_str("\n");
            }
            TypedMeasurement::ValueHistogram(label, summary) => {
                let label = label.replace('.', "_");
                output.push_str("# TYPE ");
                output.push_str(label.as_str());
                output.push_str(" summary\n");
                for (percentile, value) in summary.measurements() {
                    output.push_str(label.as_str());
                    output.push_str("{quantile=\"");
                    output.push_str(percentile.as_quantile().to_string().as_str());
                    output.push_str("\"} ");
                    output.push_str(value.to_string().as_str());
                    output.push_str("\n");
                }
                output.push_str(label.as_str());
                output.push_str("_sum ");
                output.push_str(summary.sum().to_string().as_str());
                output.push_str("\n");
                output.push_str(label.as_str());
                output.push_str("_count ");
                output.push_str(summary.count().to_string().as_str());
                output.push_str("\n");
            }
        }
    }

    Ok(output)
}
