pub mod logs;


use reqwest::{Client, Method};
use serde::Deserialize;


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

async fn get_payloads_agent(endpoint: &str) -> Payloads {
    get_fakeintake_payloads(&fake_intake_agent_endpoint(), endpoint).await
}

async fn get_payloads_vector(endpoint: &str) -> Payloads {
    get_fakeintake_payloads(&fake_intake_vector_endpoint(), endpoint).await
}
