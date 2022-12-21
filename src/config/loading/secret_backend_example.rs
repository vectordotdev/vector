use std::{
    collections::HashMap,
    io::{stdin, BufReader},
};

use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct ExecQuery {
    version: String,
    secrets: Vec<String>,
}

#[derive(Serialize)]
struct ExecResponse {
    value: String,
    error: Option<String>,
}

// Naive sample implementation the secret API and return '<secret_name>.decoded' for every requested secret/
// Used for behaviour tests.
#[tokio::main]
async fn main() {
    let stdin = BufReader::new(stdin());
    let query: ExecQuery = serde_json::from_reader(stdin).unwrap();
    if query.version != "1.0" {
        panic!("unsupported version: {}", query.version);
    }
    let response = query
        .secrets
        .into_iter()
        .map(|secret| {
            (
                secret.clone(),
                ExecResponse {
                    value: format!("{}.retrieved", secret),
                    error: None,
                },
            )
        })
        .collect::<HashMap<String, ExecResponse>>();
    #[allow(clippy::print_stdout)]
    {
        print!("{}", serde_json::to_string_pretty(&response).unwrap());
    }
}
