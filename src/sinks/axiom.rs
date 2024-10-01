use vector_lib::codecs::encoding::FramingConfig;
use vector_lib::codecs::encoding::JsonSerializerConfig;
use vector_lib::codecs::encoding::JsonSerializerOptions;
use vector_lib::codecs::encoding::SerializerConfig;
use vector_lib::codecs::MetricTagValues;
use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;

use crate::{
    codecs::{EncodingConfigWithFraming, Transformer},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    http::Auth as HttpAuthConfig,
    sinks::{
        http::config::{HttpMethod, HttpSinkConfig},
        util::{
            http::RequestConfig, BatchConfig, Compression, RealtimeSizeBasedDefaultBatchSettings,
        },
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
    #[configurable(metadata(docs::examples = "vector_rocks"))]
    dataset: String,

    #[configurable(derived)]
    #[serde(default)]
    request: RequestConfig,

    /// The compression algorithm to use.
    ///
    /// Supported values: `none` ( not recommended ), `zstd` ( recommended, default ), `gzip`, `deflate`
    #[configurable(derived)]
    #[serde(default = "Compression::zstd_default")]
    compression: Compression,

    /// The TLS settings for the connection.
    ///
    /// Optional, constrains TLS settings for this sink.
    #[configurable(derived)]
    tls: Option<TlsConfig>,

    /// The batch settings for the sink.
    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    /// Controls how acknowledgements are handled for this sink.
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
        if let Some(org_id) = &self.org_id {
            // NOTE: Only add the org id header if an org id is provided
            request
                .headers
                .insert("X-Axiom-Org-Id".to_string(), org_id.clone());
        }

        // Axiom has a custom high-performance database that can be ingested
        // into using the native HTTP ingest endpoint. This configuration wraps
        // the vector HTTP sink with the necessary adjustments to send data
        // to Axiom, whilst keeping the configuration simple and easy to use
        // and maintenance of the vector axiom sink to a minimum.
        //
        let http_sink_config = HttpSinkConfig {
            uri: self.build_endpoint().try_into()?,
            compression: self.compression,
            auth: Some(HttpAuthConfig::Bearer {
                token: self.token.clone(),
            }),
            method: HttpMethod::Post,
            tls: self.tls.clone(),
            request,
            acknowledgements: self.acknowledgements,
            batch: self.batch,
            headers: None,
            encoding: EncodingConfigWithFraming::new(
                Some(FramingConfig::NewlineDelimited),
                SerializerConfig::Json(JsonSerializerConfig {
                    metric_tag_values: MetricTagValues::Single,
                    options: JsonSerializerOptions { pretty: false }, // Minified JSON
                }),
                Transformer::default(),
            ),
            payload_prefix: "".into(), // Always newline delimited JSON
            payload_suffix: "".into(), // Always newline delimited JSON
        };

        http_sink_config.build(cx).await
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

        // NOTE trim any trailing slashes to avoid redundant rewriting or 301 redirects from intermediate proxies
        // NOTE Most axiom users will not need to configure a url, this is for the other 1%
        let url = url.trim_end_matches('/');

        format!("{}/v1/datasets/{}/ingest", url, self.dataset)
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::AxiomConfig>();

        let config = super::AxiomConfig {
            url: Some("https://axiom.my-domain.com///".to_string()),
            org_id: None,
            dataset: "vector_rocks".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(
            endpoint,
            "https://axiom.my-domain.com/v1/datasets/vector_rocks/ingest"
        );
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
        let org_id = env::var("AXIOM_ORG_ID").unwrap();

        let cx = SinkContext::default();

        let config = AxiomConfig {
            url: Some(url.clone()),
            token: token.clone().into(),
            dataset: dataset.clone(),
            org_id: Some(org_id.clone()),
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
            .header("X-Axiom-Org-Id", org_id)
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
