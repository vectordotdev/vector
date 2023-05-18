use indoc::indoc;
use tokio::{
    sync::mpsc,
    time::{sleep, Duration},
};

use vector::{
    config::ConfigBuilder,
    test_util::{start_topology, trace_init},
    topology::RunningTopology,
};

use super::*;

async fn start_vector() -> (
    RunningTopology,
    (mpsc::UnboundedSender<()>, mpsc::UnboundedReceiver<()>),
) {
    let dd_agent_address = format!("0.0.0.0:{}", vector_receive_port());

    let dd_logs_endpoint = fake_intake_vector_endpoint();

    let builder: ConfigBuilder = toml::from_str(&format!(
        indoc! {r#"
        [sources.dd_agent]
        type = "datadog_agent"
        multiple_outputs = true
        disable_metrics = true
        disable_traces = true
        address = "{}"

        [sinks.dd_logs]
        type = "datadog_logs"
        inputs = ["dd_agent.logs"]
        default_api_key = "unused"
        endpoint = "{}"
    "#},
        dd_agent_address, dd_logs_endpoint,
    ))
    .expect("toml parsing should not fail");

    let config = builder.build().expect("building config should not fail");

    let (topology, shutdown) = start_topology(config, false).await;

    println!("Started vector.");

    (topology, shutdown)
}

#[tokio::test]
async fn test_logs() {
    trace_init();

    println!("foo test");

    // panics if vector errors during startup
    let (_topology, _shutdown) = start_vector().await;

    // TODO there hopefully is a way to configure the flushing of metrics such that we don't have
    // to wait statically for so long here.
    sleep(Duration::from_secs(25)).await;

    let logs_endpoint = "/api/v2/logs";
    let _agent_payloads = get_payloads_agent(&logs_endpoint).await;

    dbg!(&_agent_payloads);

    let _vector_payloads = get_payloads_vector(&logs_endpoint).await;

    dbg!(&_vector_payloads);

    assert!(true);
}
