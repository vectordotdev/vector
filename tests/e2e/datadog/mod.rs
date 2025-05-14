pub mod logs;
pub mod metrics;

use reqwest::{Client, Method};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;

fn fake_intake_vector_address() -> String {
    std::env::var("FAKE_INTAKE_VECTOR_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:8082".to_string())
}

fn fake_intake_agent_address() -> String {
    std::env::var("FAKE_INTAKE_AGENT_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:8083".to_string())
}

#[derive(Deserialize, Debug)]
struct FakeIntakePayload<D> {
    // When string, base64 encoded
    data: D,
    #[serde(rename = "encoding")]
    _encoding: String,
    #[serde(rename = "timestamp")]
    _timestamp: String,
}

type FakeIntakePayloadJson = FakeIntakePayload<Value>;

type FakeIntakePayloadRaw = FakeIntakePayload<String>;

trait FakeIntakeResponseT {
    fn build_url(base: &str, endpoint: &str) -> String;
}

#[derive(Deserialize, Debug)]
struct FakeIntakeResponse<P> {
    payloads: Vec<P>,
}

type FakeIntakeResponseJson = FakeIntakeResponse<FakeIntakePayloadJson>;

impl FakeIntakeResponseT for FakeIntakeResponseJson {
    fn build_url(base: &str, endpoint: &str) -> String {
        format!(
            "{}/fakeintake/payloads?endpoint={}&format=json",
            base, endpoint,
        )
    }
}

type FakeIntakeResponseRaw = FakeIntakeResponse<FakeIntakePayloadRaw>;

impl FakeIntakeResponseT for FakeIntakeResponseRaw {
    fn build_url(base: &str, endpoint: &str) -> String {
        format!("{}/fakeintake/payloads?endpoint={}", base, endpoint,)
    }
}

async fn get_fakeintake_payloads<R>(base: &str, endpoint: &str) -> R
where
    R: FakeIntakeResponseT + DeserializeOwned,
{
    let url = &R::build_url(base, endpoint);
    Client::new()
        .request(Method::GET, url)
        .send()
        .await
        .unwrap_or_else(|_| panic!("Sending GET request to {} failed", url))
        .json::<R>()
        .await
        .expect("Parsing fakeintake payloads failed")
}
