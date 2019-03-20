extern crate futures;
extern crate hyper;
#[macro_use]
extern crate tokio_trace;
extern crate tokio;
extern crate tokio_trace_fmt;
extern crate tokio_trace_futures;

use futures::future;
use hyper::rt::{Future, Stream};
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::{Body, Method, Request, Response, StatusCode};

use std::str;

use tokio_trace::field;
use tokio_trace_futures::{Instrument, Instrumented};

type BoxFut = Box<Future<Item = Response<Body>, Error = hyper::Error> + Send>;

fn echo(req: Request<Body>) -> Instrumented<BoxFut> {
    span!(
        "request",
        method = &field::debug(req.method()),
        uri = &field::debug(req.uri()),
        headers = &field::debug(req.headers())
    )
    .enter(|| {
        info!("received request");
        let mut response = Response::new(Body::empty());

        let (rsp_span, fut): (_, BoxFut) = match (req.method(), req.uri().path()) {
            // Serve some instructions at /
            (&Method::GET, "/") => {
                const BODY: &'static str = "Try POSTing data to /echo";
                *response.body_mut() = Body::from(BODY);
                (
                    span!("response", body = &field::display(&BODY)),
                    Box::new(future::ok(response)),
                )
            }

            // Simply echo the body back to the client.
            (&Method::POST, "/echo") => {
                let body = req.into_body();
                let span = span!("response", response_kind = &"echo");
                *response.body_mut() = body;
                (span, Box::new(future::ok(response)))
            }

            // Convert to uppercase before sending back to client.
            (&Method::POST, "/echo/uppercase") => {
                let mapping = req.into_body().map(|chunk| {
                    let upper = chunk
                        .iter()
                        .map(|byte| byte.to_ascii_uppercase())
                        .collect::<Vec<u8>>();
                    debug!(
                        {
                            chunk = field::debug(str::from_utf8(&chunk[..])),
                            uppercased = field::debug(str::from_utf8(&upper[..]))
                        },
                        "uppercased request body"
                    );
                    upper
                });

                *response.body_mut() = Body::wrap_stream(mapping);
                (
                    span!("response", response_kind = "uppercase"),
                    Box::new(future::ok(response)),
                )
            }

            // Reverse the entire body before sending back to the client.
            //
            // Since we don't know the end yet, we can't simply stream
            // the chunks as they arrive. So, this returns a different
            // future, waiting on concatenating the full body, so that
            // it can be reversed. Only then can we return a `Response`.
            (&Method::POST, "/echo/reversed") => {
                let mut span = span!("response", response_kind = "reversed");
                let reversed = span.enter(|| {
                    req.into_body().concat2().map(move |chunk| {
                        let body = chunk.iter().rev().cloned().collect::<Vec<u8>>();
                        debug!(
                            {
                                chunk = field::debug(str::from_utf8(&chunk[..])),
                                body = field::debug(str::from_utf8(&body[..]))
                            },
                            "reversed request body");
                        *response.body_mut() = Body::from(body);
                        response
                    })
                });
                (span, Box::new(reversed))
            }

            // The 404 Not Found route...
            _ => {
                *response.status_mut() = StatusCode::NOT_FOUND;
                (
                    span!(
                        "response",
                        body = &field::debug(()),
                        status = &field::debug(&StatusCode::NOT_FOUND)
                    ),
                    Box::new(future::ok(response)),
                )
            }
        };

        fut.instrument(rsp_span)
    })
}

fn main() {
    use hotmic::Receiver;
    let mut receiver = Receiver::builder().build();
    let sink = receiver.get_sink();
    let controller = receiver.get_controller();

    std::thread::spawn(move || {
        receiver.run();
    });

    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));

        let snapshot = controller.get_snapshot().unwrap();
        let raw_snap = serde_json::to_string_pretty(&snapshot).unwrap();

        println!("Metrics snapshot: {}", raw_snap);
    });

    let subscriber = tokio_trace_fmt::FmtSubscriber::builder().full().finish();
    tokio_trace_env_logger::try_init().expect("init log adapter");

    let subscriber = trace_metrics::MetricsSubscriber::new(subscriber, sink);

    tokio_trace::subscriber::with_default(subscriber, || {
        let addr: ::std::net::SocketAddr = ([127, 0, 0, 1], 3000).into();
        let mut server_span = span!("server", local = &field::debug(addr));
        let server = tokio::net::TcpListener::bind(&addr)
            .expect("bind")
            .incoming()
            .fold(Http::new(), move |http, sock| {
                let span = span!(
                    "connection",
                    remote = &field::debug(&sock.peer_addr().unwrap())
                );
                hyper::rt::spawn(
                    http.serve_connection(sock, service_fn(echo))
                        .and_then(|_| {
                            println!("Connection is done!");
                            Ok(())
                        })
                        .map_err(|e| {
                            error!({ error = field::display(e) }, "serve error");
                        })
                        .instrument(span),
                );
                Ok::<_, ::std::io::Error>(http)
            })
            .map(|_| ())
            .map_err(|e| {
                error!({ error = field::display(e) }, "server error");
            })
            .instrument(server_span.clone());
        server_span.enter(|| {
            info!("listening...");
            hyper::rt::run(server);
        });
    })
}
