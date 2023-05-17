use indoc::indoc;
use reqwest::{Client, Method};
use serde::Deserialize;
use tokio::sync::mpsc;

use vector::{config::ConfigBuilder, test_util::start_topology, topology::RunningTopology};

fn vector_receive_port() -> u16 {
    std::env::var("VECTOR_RECEIVE_PORT")
        .unwrap_or_else(|_| "8081".to_string())
        .parse::<u16>()
        .unwrap()
}

fn fake_intake_vector_endpoint() -> String {
    std::env::var("FAKE_INTAKE_VECTOR_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:8082".to_string())
}

fn fake_intake_agent_endpoint() -> String {
    std::env::var("FAKE_INTAKE_AGENT_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:8083".to_string())
}

// Fakeintake response
#[derive(Deserialize, Debug)]
struct Payloads {
    payloads: Vec<Payload>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct Payload {
    // base64 encoded
    data: String,
    encoding: String,
    timestamp: String,
}

async fn get_fakeintake_payloads(base: &str, endpoint: &str) -> Payloads {
    let url = format!("{}/fakeintake/payloads?endpoint={}", base, endpoint,);

    Client::new()
        .request(Method::GET, &url)
        .send()
        .await
        .unwrap_or_else(|_| panic!("Sending GET request to {} failed", &url))
        .json::<Payloads>()
        .await
        .expect("Parsing fakeintake payloads failed")
}

async fn get_payloads_agent() -> Payloads {
    get_fakeintake_payloads(&fake_intake_agent_endpoint(), "TODO logs endpoint").await
}

async fn get_payloads_vector() -> Payloads {
    get_fakeintake_payloads(&fake_intake_vector_endpoint(), "TODO logs endpoint").await
}

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
    println!("foo test");

    // panics if vector errors during startup
    let (_topology, _shutdown) = start_vector().await;

    let _agent_payloads = get_payloads_agent().await;

    let _vector_payloads = get_payloads_vector().await;

    assert!(true);
}
