pub mod logs;

use reqwest::{Client, Method};
use serde::Deserialize;

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
    let url = format!(
        "{}/fakeintake/payloads?endpoint={}",
        // "{}/fakeintake/payloads?endpoint={}&format=json",
        base,
        endpoint,
    );

    let res = Client::new()
        .request(Method::GET, &url)
        .send()
        .await
        .unwrap_or_else(|_| panic!("Sending GET request to {} failed", &url))
        //.text()
        .json::<Payloads>()
        .await
        .expect("Parsing fakeintake payloads failed");

    res

    //println!("body= {:?}", res);

    //Payloads { payloads: vec![] }
}

async fn get_payloads_agent(endpoint: &str) -> Vec<Payload> {
    let mut raw_payloads = get_fakeintake_payloads(&fake_intake_agent_endpoint(), endpoint)
        .await
        .payloads;

    // Not sure what this is but the logs endpoint receives some empty payload in the beginning
    if raw_payloads.len() > 0 && endpoint == "/api/v2/logs" {
        if raw_payloads[0].data == "" && raw_payloads[0].encoding == "" {
            raw_payloads.remove(0);
            return raw_payloads;
        }
    }

    raw_payloads
}

async fn get_payloads_vector(endpoint: &str) -> Vec<Payload> {
    get_fakeintake_payloads(&fake_intake_vector_endpoint(), endpoint)
        .await
        .payloads
}
