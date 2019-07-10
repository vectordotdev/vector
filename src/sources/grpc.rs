use crate::{
    event::{
        proto::grpc::{server, WriteEventsRequest, WriteEventsResponse},
        Event,
    },
    topology::config::SourceConfig,
};
use futures::{stream::iter_ok, sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_grpc::{Code, Request, Response, Status};
use tower_hyper::server::{Http, Server};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct GrpcConfig {
    pub address: SocketAddr,
}

#[typetag::serde(name = "grpc")]
impl SourceConfig for GrpcConfig {
    fn build(&self, out: mpsc::Sender<Event>) -> Result<super::Source, String> {
        let svc = server::VectorServiceServer::new(GrpcService { out });
        let mut server = Server::new(svc);

        let http = Http::new().http2_only(true).clone();

        let bind = TcpListener::bind(&self.address).map_err(|e| format!("GrpcBindError: {}", e))?;

        let serve = bind
            .incoming()
            .for_each(move |sock| {
                if let Err(e) = sock.set_nodelay(true) {
                    return Err(e);
                }

                let serve = server.serve_with(sock, http.clone());
                tokio::spawn(serve.map_err(|e| error!("hyper error: {:?}", e)));

                Ok(())
            })
            .map_err(|e| error!("Connection error: {}", e));

        Ok(Box::new(serve))
    }
}

#[derive(Debug, Clone)]
pub struct GrpcService {
    out: mpsc::Sender<Event>,
}

impl server::VectorService for GrpcService {
    type WriteEventsFuture =
        Box<dyn Future<Item = Response<WriteEventsResponse>, Error = Status> + Send + 'static>;

    fn write_events(&mut self, request: Request<WriteEventsRequest>) -> Self::WriteEventsFuture {
        let body = request.into_inner();
        let events = iter_ok(body.events).map(|e| Event::from(e));

        let fut = self.out.clone().send_all(events).then(|res| match res {
            Ok(_) => Ok(Response::new(WriteEventsResponse {})),
            Err(_) => {
                error!("Sender dropped, most likely in the process of shutting down.");
                Err(Status::new(Code::Unknown, "Server shutting down"))
            }
        });

        Box::new(fut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{
            proto::grpc::{client::VectorService, WriteEventsRequest},
            Event,
        },
        test_util::{next_addr, random_events_with_stream, CollectCurrent},
    };
    use futures::{sync::mpsc, Future};
    use hyper::client::connect::HttpConnector;
    use tokio::runtime::current_thread::Runtime;
    use tower::MakeService;
    use tower_grpc::{BoxBody, Code, Request, Status};
    use tower_hyper::client::{self, Connect, Connection};
    use tower_request_modifier::RequestModifier;

    #[test]
    fn write_events() {
        let mut rt = Runtime::new().unwrap();

        let address = next_addr();
        let grpc = GrpcConfig { address };

        let (tx, rx) = mpsc::channel(1000);
        let fut = grpc.build(tx).unwrap();

        rt.spawn(fut);

        let (events, _) = random_events_with_stream(100, 100);

        let events = events
            .into_iter()
            .map(|s| Event::from(s))
            .collect::<Vec<_>>();

        rt.block_on(make_request(address, events.clone())).unwrap();

        let (_, output) = CollectCurrent::new(rx).wait().unwrap();

        assert_eq!(events, output);
    }

    #[test]
    fn write_events_shutdown() {
        let mut rt = Runtime::new().unwrap();

        let address = next_addr();
        let grpc = GrpcConfig { address };

        let (tx, rx) = mpsc::channel(1000);
        let fut = grpc.build(tx).unwrap();

        rt.spawn(fut);

        let (events, _) = random_events_with_stream(100, 100);

        let events = events
            .into_iter()
            .map(|s| Event::from(s))
            .collect::<Vec<_>>();

        drop(rx);
        let err = rt
            .block_on(make_request(address, events.clone()))
            .unwrap_err();

        assert_eq!(err.code(), Code::Unknown);
        assert_eq!(err.message(), "Server shutting down");
    }

    fn make_request(
        addr: SocketAddr,
        events: Vec<Event>,
    ) -> impl Future<Item = (), Error = Status> {
        let events = events.into_iter().map(|e| e.into()).collect();

        connect(addr)
            .map_err(|s| Status::new(Code::Unavailable, s))
            .and_then(move |mut svc| {
                let req = Request::new(WriteEventsRequest { events });
                svc.write_events(req).map(drop)
            })
    }

    fn connect(
        addr: SocketAddr,
    ) -> impl Future<Item = VectorService<RequestModifier<Connection<BoxBody>, BoxBody>>, Error = String>
    {
        let uri: http::Uri = format!("http://{}", addr).parse().unwrap();

        let dst = hyper::client::connect::Destination::try_from_uri(uri.clone()).unwrap();
        let connector = tower_hyper::util::Connector::new(HttpConnector::new(4));
        let settings = client::Builder::new().http2_only(true).clone();
        let mut make_client = Connect::with_builder(connector, settings);

        make_client
            .make_service(dst)
            .map_err(|e| format!("{}", e))
            .and_then(|conn| {
                let conn = tower_request_modifier::Builder::new()
                    .set_origin(uri)
                    .build(conn)
                    .unwrap();

                VectorService::new(conn)
                    .ready()
                    .map_err(|e| format!("{}", e))
            })
    }
}
