use futures::{future, Future, Poll, Stream};
use hotmic::{Controller, Receiver, Sink, Snapshot};
use hyper::{Body, Request, Response};
use std::fmt;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_trace::field::{Field, Visit};
use tokio_trace_fmt::default::Recorder;
use tokio_trace_fmt::NewVisitor;
use tower_hyper::body::LiftBody;
use tower_hyper::server::Server;
use tower_service::Service;

pub struct NewMetricRecorder {
    sink: Sink<String>,
}

pub struct MetricsServer {
    controller: Controller,
}

pub struct MetricVisitor<'a> {
    recorder: Recorder<'a>,
    sink: Sink<String>,
}

pub struct MetricsServerSvc {
    snapshot: Snapshot,
}

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

impl NewMetricRecorder {
    pub fn new() -> (MetricsServer, Self) {
        let mut receiver = Receiver::builder().build();
        let controller = receiver.get_controller();
        let sink = receiver.get_sink();

        std::thread::spawn(move || {
            receiver.run();
        });

        let visitor = NewMetricRecorder { sink };
        let server = MetricsServer { controller };

        (server, visitor)
    }
}

impl<'a> NewVisitor<'a> for NewMetricRecorder {
    type Visitor = MetricVisitor<'a>;

    #[inline]
    fn make(&self, writer: &'a mut fmt::Write, is_empty: bool) -> Self::Visitor {
        let recorder = Recorder::new(writer, is_empty);
        let sink = self.sink.clone();
        MetricVisitor::new(recorder, sink)
    }
}

impl<'a> MetricVisitor<'a> {
    pub fn new(recorder: Recorder<'a>, sink: Sink<String>) -> Self {
        MetricVisitor { recorder, sink }
    }
}

impl<'a> Visit for MetricVisitor<'a> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.recorder.record_str(field, value)
    }

    fn record_debug(&mut self, field: &Field, value: &fmt::Debug) {
        self.recorder.record_debug(field, value)
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name().contains("counter") {
            self.sink
                .update_count(field.name().to_string(), value as i64);
        } else {
            self.recorder.record_u64(field, value);
        }
    }
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
