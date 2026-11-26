//! Inventory service: a server span per request, a child span under another service name, and a
//! server error for one order.

use std::{convert::Infallible, time::Duration};

use http_body_util::Full;
use hyper::{
    Request, Response, StatusCode, body::Bytes, body::Incoming, server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use opentelemetry::{
    KeyValue, global,
    trace::{Span, SpanKind, Status, TraceContextExt, Tracer},
};
use opentelemetry_http::HeaderExtractor;
use tokio::{net::TcpListener, signal::unix};

async fn stock(request: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let tracer = global::tracer("inventory");
    let parent = global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(request.headers()))
    });
    let mut server = tracer
        .span_builder("http.request")
        .with_kind(SpanKind::Server)
        .with_attributes([
            KeyValue::new("http.request.method", "GET"),
            KeyValue::new("http.route", "/stock/{order}"),
            KeyValue::new("resource.name", "GET /stock/{order}"),
        ])
        .start_with_context(&tracer, &parent);
    tracer
        .span_builder("stock.query")
        .with_attributes([
            // A different service in the same local trace makes a multi-service chunk.
            KeyValue::new("service.name", "inventory-db"),
            KeyValue::new("operation.name", "stock.query"),
            KeyValue::new("span.type", "sql"),
            KeyValue::new("resource.name", "SELECT stock"),
            KeyValue::new("db.rows", 7),
        ])
        .start_with_context(
            &tracer,
            &parent.with_remote_span_context(server.span_context().clone()),
        )
        .end();

    let status = if request.uri().path().ends_with("/4") {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::OK
    };
    server.set_attribute(KeyValue::new(
        "http.response.status_code",
        i64::from(status.as_u16()),
    ));
    if status.is_server_error() {
        server.set_status(Status::error("out of stock"));
    }
    server.end();
    let mut response = Response::new(Full::new(Bytes::from_static(b"ok")));
    *response.status_mut() = status;
    Ok(response)
}

#[tokio::main]
async fn main() {
    let provider = datadog_opentelemetry::tracing().init();
    let listener = TcpListener::bind("0.0.0.0:8080").await.expect("bind :8080");
    let mut sigterm = unix::signal(unix::SignalKind::terminate()).expect("SIGTERM handler");
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted.expect("accept connection");
                tokio::spawn(http1::Builder::new().serve_connection(TokioIo::new(stream), service_fn(stock)));
            }
            _ = sigterm.recv() => break,
        }
    }
    // Flush buffered spans on `docker stop`.
    provider
        .shutdown_with_timeout(Duration::from_secs(5))
        .expect("flush spans");
}
