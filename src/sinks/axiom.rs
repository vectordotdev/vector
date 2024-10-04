use std::collections::HashMap;

use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;

use crate::{
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{
        elasticsearch::{ElasticsearchApiVersion, ElasticsearchAuthConfig, ElasticsearchConfig},
        util::{http::RequestConfig, Compression},
        Healthcheck, VectorSink,
    },
    tls::TlsConfig,
};

static CLOUD_URL: &str = "https://api.axiom.co";

/// Configuration for the `axiom` sink.
#[configurable_component(sink("axiom", "Deliver log events to Axiom."))]
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
    #[configurable(metadata(docs::examples = "${AXIOM_DATASET}"))]
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
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for AxiomConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"token = "${AXIOM_TOKEN}"
            dataset = "${AXIOM_DATASET}"
            url = "${AXIOM_URL}"
            org_id = "${AXIOM_ORG_ID}""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "axiom")]
impl SinkConfig for AxiomConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let mut request = self.request.clone();
        request.headers.insert(
            "X-Axiom-Org-Id".to_string(),
            self.org_id.clone().unwrap_or_default(),
        );
        let query = HashMap::from([("timestamp-field".to_string(), "@timestamp".to_string())]);

        // Axiom has a custom high-performance database that can be ingested
        // into using our HTTP endpoints, including one compatible with the
        // Elasticsearch Bulk API.
        // This configuration wraps the Elasticsearch config to minimize the
        // amount of code.
        let elasticsearch_config = ElasticsearchConfig {
            endpoints: vec![self.build_endpoint()],
            compression: self.compression,
            auth: Some(ElasticsearchAuthConfig::Basic {
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

        format!("{}/v1/datasets/{}/elastic", url, self.dataset)
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
    use serde::{Deserialize, Serialize};
    use std::env;
    use vector_lib::event::{BatchNotifier, BatchStatus, Event, LogEvent};

    use super::*;
    use crate::{
        config::SinkContext,
        sinks::axiom::AxiomConfig,
        test_util::components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS},
    };

    #[tokio::test]
    async fn axiom_logs_put_data() {
        let client = reqwest::Client::new();
        let url = env::var("AXIOM_URL").unwrap();
        let token = env::var("AXIOM_TOKEN").expect("AXIOM_TOKEN environment variable to be set");
        assert!(!token.is_empty(), "$AXIOM_TOKEN required");
        let dataset = env::var("AXIOM_DATASET").unwrap();

        let cx = SinkContext::default();

        let config = AxiomConfig {
            url: Some(url.clone()),
            token: token.clone().into(),
            dataset: dataset.clone(),
            ..Default::default()
        };

        // create unique test id so tests can run in parallel
        let test_id = uuid::Uuid::new_v4().to_string();

        let (sink, _) = config.build(cx).await.unwrap();

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();

        let mut event1 = LogEvent::from("message_1").with_batch_notifier(&batch);
        event1.insert("host", "aws.cloud.eur");
        event1.insert("source_type", "file");
        event1.insert("test_id", test_id.clone());

        let mut event2 = LogEvent::from("message_2").with_batch_notifier(&batch);
        event2.insert("host", "aws.cloud.eur");
        event2.insert("source_type", "file");
        event2.insert("test_id", test_id.clone());

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
            apl: format!(
                "['{}'] | where test_id == '{}' | order by _time desc | limit 2",
                dataset, test_id
            ),
            start_time: Utc::now() - Duration::minutes(10),
            end_time: Utc::now() + Duration::minutes(10),
        };
        let query_res: QueryResponse = client
            .post(format!("{}/v1/datasets/_apl?format=legacy", url))
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
