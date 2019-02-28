use bytes::Buf;
use futures::{stream, sync::mpsc, Future, Sink, Stream};
use headers::{Authorization, HeaderMapExt};
use hyper::service::service_fn_ok;
use hyper::{Body, Request, Response, Server};
use router::{
    sinks::http::HttpSinkConfig,
    test_util::{next_addr, random_lines, shutdown_on_idle},
    topology::config::SinkConfig,
};
use std::io::{BufRead, BufReader};

#[test]
fn test_http_happy_path() {
    router::setup_logger();

    let num_lines = 1000;

    let in_addr = next_addr();

    let config = r#"
        uri = "http://$IN_ADDR/frames"
        user = "waldo"
        password = "hunter2"
    "#
    .replace("$IN_ADDR", &format!("{}", in_addr));

    let config: HttpSinkConfig = toml::from_str(&config).unwrap();

    let (sink, _healthcheck) = config.build().unwrap();

    let (tx, rx) = mpsc::unbounded();
    let service = move || {
        let tx = tx.clone();
        service_fn_ok(move |req: Request<Body>| {
            let (parts, body) = req.into_parts();

            let tx = tx.clone();
            tokio::spawn(
                body.concat2()
                    .map_err(|e| panic!(e))
                    .and_then(|body| tx.send((parts, body)))
                    .map(|_| ())
                    .map_err(|e| panic!(e)),
            );

            Response::new(Body::empty())
        })
    };

    let (trigger, tripwire) = stream_cancel::Tripwire::new();
    let server = Server::bind(&in_addr)
        .serve(service)
        .with_graceful_shutdown(tripwire)
        .map_err(|e| panic!("server error: {}", e));

    let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();

    let pump = sink.send_all(stream::iter_ok::<_, ()>(
        input_lines
            .clone()
            .into_iter()
            .map(|line| router::Record::from(line)),
    ));

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(server);

    let (mut sink, _) = rt.block_on(pump).unwrap();
    rt.block_on(futures::future::poll_fn(move || sink.close()))
        .unwrap();

    drop(trigger);

    let output_lines = rx
        .wait()
        .map(Result::unwrap)
        .map(|(parts, body)| {
            assert_eq!(hyper::Method::POST, parts.method);
            assert_eq!("/frames", parts.uri.path());
            assert_eq!(
                Some(Authorization::basic("waldo", "hunter2")),
                parts.headers.typed_get()
            );
            body
        })
        .map(hyper::Chunk::reader)
        .map(flate2::read::GzDecoder::new)
        .map(BufReader::new)
        .flat_map(BufRead::lines)
        .map(Result::unwrap)
        .map(|s| {
            let val: serde_json::Value = serde_json::from_str(&s).unwrap();
            val.get("msg").unwrap().as_str().unwrap().to_owned()
        })
        .collect::<Vec<_>>();

    shutdown_on_idle(rt);

    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}
