use std::collections::HashMap;

use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;

use crate::{
    config::{
        log_schema, AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig,
        SinkContext,
    },
    sinks::{
        elasticsearch::{ElasticsearchApiVersion, ElasticsearchAuth, ElasticsearchConfig},
        util::{http::RequestConfig, Compression},
        Healthcheck, VectorSink,
    },
    tls::TlsConfig,
};

static CLOUD_URL: &str = "https://cloud.axiom.co";

/// Configuration for the `axiom` sink.
#[configurable_component(sink("axiom"))]
#[derive(Clone, Debug, Default)]
pub struct AxiomConfig {
    /// URI of the Axiom endpoint to send data to.
    ///
    /// Only required if not using Axiom Cloud.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "https://axiom.my-domain.com"))]
    #[configurable(metadata(docs::examples = "${AXIOM_URL}"))]
    url: Option<String>,

    /// The Axiom organization ID.
    ///
    /// Only required when using personal tokens.
    #[configurable(metadata(docs::examples = "${AXIOM_ORG_ID}"))]
    #[configurable(metadata(docs::examples = "123abc"))]
    org_id: Option<String>,

    /// The Axiom API token.
    #[configurable(metadata(docs::examples = "${AXIOM_TOKEN}"))]
    #[configurable(metadata(docs::examples = "123abc"))]
    token: SensitiveString,

    /// The Axiom dataset to write to.
    #[configurable(metadata(docs::examples = "vector.dev"))]
    dataset: String,

    #[configurable(derived)]
    #[serde(default)]
    request: RequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    compression: Compression,

    #[configurable(derived)]
    tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for AxiomConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"token = "${AXIOM_TOKEN}"
            dataset = "my-dataset"
            url = "${AXIOM_URL}"
            org_id = "${AXIOM_ORG_ID}""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for AxiomConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let mut request = self.request.clone();
        request.headers.insert(
            "X-Axiom-Org-Id".to_string(),
            self.org_id.clone().unwrap_or_default(),
        );
        let mut query = HashMap::with_capacity(1);
        query.insert(
            "timestamp-field".to_string(),
            log_schema().timestamp_key().to_string(),
        );

        // Axiom has a custom high-performance database that can be ingested
        // into using our HTTP endpoints, including one compatible with the
        // Elasticsearch Bulk API.
        // This configuration wraps the Elasticsearch config to minimize the
        // amount of code.
        let elasticsearch_config = ElasticsearchConfig {
            endpoints: vec![self.build_endpoint()],
            compression: self.compression,
            auth: Some(ElasticsearchAuth::Basic {
                user: "axiom".to_string(),
                password: self.token.clone(),
            }),
            query: Some(query),
            tls: self.tls.clone(),
            request,
            api_version: ElasticsearchApiVersion::V6,
            ..Default::default()
        };

        elasticsearch_config.build(cx).await
    }

    fn input(&self) -> Input {
        Input::new(DataType::Metric | DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl AxiomConfig {
    fn build_endpoint(&self) -> String {
        let url = if let Some(url) = self.url.as_ref() {
            url.clone()
        } else {
            CLOUD_URL.to_string()
        };

        format!("{}/api/v1/datasets/{}/elastic", url, self.dataset)
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::AxiomConfig>();
    }
}

#[cfg(feature = "axiom-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use chrono::{DateTime, Duration, Utc};
    use futures::stream;
    use http::StatusCode;
    use serde::{Deserialize, Serialize};
    use std::env;
    use tokio::time;
    use vector_core::event::{BatchNotifier, BatchStatus, Event, LogEvent};

    use super::*;
    use crate::{
        config::SinkContext,
        sinks::axiom::AxiomConfig,
        test_util::{
            components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS},
            wait_for_duration,
        },
    };

    #[tokio::test]
    async fn axiom_logs_put_data() {
        // Wait until deployment is ready
        wait_for_duration(
            || async {
                let url = env::var("AXIOM_URL").unwrap();
                reqwest::get(url)
                    .await
                    .map(|res| res.status() == StatusCode::OK)
                    .unwrap_or(false)
            },
            time::Duration::from_secs(30),
        )
        .await;

        let client = reqwest::Client::new();
        let url = env::var("AXIOM_URL").unwrap();

        // Axiom credentials
        let email = "info@axiom.co".to_string();
        let password = "vector-is-cool".to_string();

        // Is the deployment already set up? Try to login and get the session
        // cookie.
        #[derive(Serialize)]
        struct LoginRequest {
            email: String,
            password: String,
        }
        let login_payload = LoginRequest {
            email: email.clone(),
            password: password.clone(),
        };
        let login_url = format!("{}/auth/signin/credentials", url);
        let login_res = client
            .post(&login_url)
            .json(&login_payload)
            .send()
            .await
            .unwrap();
        let session_cookie = if login_res.status() == StatusCode::OK {
            Some(
                login_res
                    .headers()
                    .get("set-cookie")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
            )
        } else {
            None
        };

        // If the deployment is not yet setup, set it up and login to get the
        // session cookie.
        #[derive(Serialize)]
        struct AuthInitRequest {
            org: String,
            name: String,
            email: String,
            password: String,
        }
        let auth_init_payload = AuthInitRequest {
            org: "vector".to_string(),
            name: "Vector".to_string(),
            email: email.clone(),
            password: password.clone(),
        };
        let session_cookie = match session_cookie {
            Some(cookie) => cookie,
            None => {
                // Try to initialize the deployment
                client
                    .post(format!("{}/auth/init", url))
                    .json(&auth_init_payload)
                    .send()
                    .await
                    .unwrap()
                    .error_for_status()
                    .unwrap();

                // Try again to log in and get the session cookie
                let login_res = client
                    .post(&login_url)
                    .json(&login_payload)
                    .send()
                    .await
                    .unwrap()
                    .error_for_status()
                    .unwrap();
                let cookie_string = login_res
                    .headers()
                    .get("set-cookie")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string();
                cookie_string.split(';').next().unwrap().to_string()
            }
        };

        // Create a token
        #[derive(Serialize)]
        struct CreateTokenRequest {
            id: String,
            name: String,
        }

        #[derive(Deserialize)]
        struct CreateTokenResponse {
            id: String,
        }

        let create_token_payload = CreateTokenRequest {
            id: "new".to_string(),
            name: "Vector Test Token".to_string(),
        };
        let create_token_res: CreateTokenResponse = client
            .post(format!("{}/api/v1/tokens/personal", url))
            .header("Cookie", session_cookie.clone())
            .json(&create_token_payload)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();

        // Get the created token
        #[derive(Deserialize)]
        struct TokenResponse {
            token: String,
        }
        let token_res: TokenResponse = client
            .get(format!(
                "{}/api/v1/tokens/personal/{}/token",
                url, create_token_res.id
            ))
            .header("Cookie", session_cookie)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        let token = token_res.token;

        #[derive(Serialize)]
        struct CreateDatasetRequest {
            name: String,
            description: String,
        }
        let dataset = "vector-test".to_string();
        let create_dataset_payload = CreateDatasetRequest {
            name: dataset.clone(),
            description: "Vector Test Dataset".to_string(),
        };
        let create_dataset_res = client
            .post(format!("{}/api/v1/datasets", url))
            .header("Authorization", format!("Bearer {}", token))
            .json(&create_dataset_payload)
            .send()
            .await
            .unwrap();
        match create_dataset_res.status() {
            StatusCode::OK => Ok(()),                                  // Created
            StatusCode::CONFLICT => Ok(()),                            // Dataset already exists
            _ => create_dataset_res.error_for_status().map(|_res| ()), // Error
        }
        .unwrap();

        let cx = SinkContext::new_test();

        let config = AxiomConfig {
            url: Some(url.clone()),
            token: token.clone().into(),
            dataset: dataset.clone(),
            ..Default::default()
        };

        let (sink, _) = config.build(cx).await.unwrap();

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();

        let mut event1 = LogEvent::from("message_1").with_batch_notifier(&batch);
        event1.insert("host", "aws.cloud.eur");
        event1.insert("source_type", "file");

        let mut event2 = LogEvent::from("message_2").with_batch_notifier(&batch);
        event2.insert("host", "aws.cloud.eur");
        event2.insert("source_type", "file");

        drop(batch);

        let events = vec![Event::Log(event1), Event::Log(event2)];

        run_and_assert_sink_compliance(sink, stream::iter(events), &HTTP_SINK_TAGS).await;

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        #[derive(Serialize)]
        struct QueryRequest {
            apl: String,
            #[serde(rename = "endTime")]
            end_time: DateTime<Utc>,
            #[serde(rename = "startTime")]
            start_time: DateTime<Utc>,
            // ...
        }

        #[derive(Deserialize, Debug)]
        struct QueryResponseMatch {
            data: serde_json::Value,
            // ...
        }

        #[derive(Deserialize, Debug)]
        struct QueryResponse {
            matches: Vec<QueryResponseMatch>,
            // ...
        }

        let query_req = QueryRequest {
            apl: format!("['{}'] | order by _time desc | limit 2", dataset),
            start_time: Utc::now() - Duration::minutes(10),
            end_time: Utc::now() + Duration::minutes(10),
        };
        let query_res: QueryResponse = client
            .post(format!("{}/api/v1/datasets/_apl?format=legacy", url))
            .header("Authorization", format!("Bearer {}", token))
            .json(&query_req)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(2, query_res.matches.len());

        let fst = match query_res.matches[0].data {
            serde_json::Value::Object(ref obj) => obj,
            _ => panic!("Unexpected value, expected object"),
        };
        // Note that we order descending, so message_2 comes first
        assert_eq!("message_2", fst.get("message").unwrap().as_str().unwrap());

        let snd = match query_res.matches[1].data {
            serde_json::Value::Object(ref obj) => obj,
            _ => panic!("Unexpected value, expected object"),
        };
        assert_eq!("message_1", snd.get("message").unwrap().as_str().unwrap());
    }
}
