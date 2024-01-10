pub mod logs;

use reqwest::{Client, Method};
use serde::Deserialize;
use serde_json::Value;

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
struct FakeIntakeResponse {
    payloads: Vec<FakeIntakePayload>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct FakeIntakePayload {
    data: Value,
    encoding: String,
    timestamp: String,
}

async fn get_fakeintake_payloads(base: &str, endpoint: &str) -> FakeIntakeResponse {
    let url = format!(
        "{}/fakeintake/payloads?endpoint={}&format=json",
        base, endpoint,
    );

    Client::new()
        .request(Method::GET, &url)
        .send()
        .await
        .unwrap_or_else(|_| panic!("Sending GET request to {} failed", &url))
        .json::<FakeIntakeResponse>()
        .await
        .expect("Parsing fakeintake payloads failed")
}

async fn get_payloads_agent(endpoint: &str) -> Vec<Value> {
    let mut raw_payloads = get_fakeintake_payloads(&fake_intake_agent_endpoint(), endpoint)
        .await
        .payloads;

    // Not sure what this is but the logs endpoint receives an empty payload in the beginning
    if raw_payloads.len() > 0 && endpoint == "/api/v2/logs" {
        raw_payloads.retain(|raw_payload| raw_payload.data.as_array().unwrap().len() != 0);
    }

    raw_payloads.into_iter().map(|raw| raw.data).collect()
}

async fn get_payloads_vector(endpoint: &str) -> Vec<Value> {
    get_fakeintake_payloads(&fake_intake_vector_endpoint(), endpoint)
        .await
        .payloads
        .into_iter()
        .map(|raw| raw.data)
        .collect()
}
