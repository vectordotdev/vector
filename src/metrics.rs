use futures::Future;
use hotmic::{
    snapshot::{Snapshot, TypedMeasurement},
    Receiver, Sink,
};
use hyper::{
    service::{make_service_fn, service_fn_ok},
    Body, Request, Response, Server,
};
use std::net::SocketAddr;

/// Create the metrics sink and provide the server Service
pub fn build(addr: &SocketAddr) -> (Sink<String>, impl Future<Item = (), Error = ()>) {
    let mut receiver = Receiver::builder().build();
    let controller = receiver.get_controller();
    let sink = receiver.get_sink();

    std::thread::spawn(move || {
        receiver.run();
    });

    let make_svc = make_service_fn(move |_| {
        let controller = controller.clone();
        service_fn_ok(move |_: Request<Body>| {
            let snapshot = controller.get_snapshot().unwrap();
            let output = process_snapshot(snapshot).unwrap();
            Response::new(Body::from(output))
        })
    });

    let server = Server::bind(&addr)
        .serve(make_svc)
        .map_err(|e| error!("metrics server error: {}", e));

    (sink, server)
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
