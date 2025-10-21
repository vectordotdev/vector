use vector_lib::{
    codecs::{
        MetricTagValues,
        encoding::{FramingConfig, JsonSerializerConfig, JsonSerializerOptions, SerializerConfig},
    },
    configurable::configurable_component,
    sensitive_string::SensitiveString,
};

use crate::{
    codecs::{EncodingConfigWithFraming, Transformer},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    http::Auth as HttpAuthConfig,
    sinks::{
        Healthcheck, VectorSink,
        http::config::{HttpMethod, HttpSinkConfig},
        util::{
            BatchConfig, Compression, RealtimeSizeBasedDefaultBatchSettings, http::RequestConfig,
        },
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
    /// If a path is provided, the URL is used as-is.
    /// If no path (or only `/`) is provided, `/v1/datasets/{dataset}/ingest` is appended for backwards compatibility.
    /// This takes precedence over `region` if both are set.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "https://api.eu.axiom.co"))]
    #[configurable(metadata(docs::examples = "http://localhost:3400/ingest"))]
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

    /// The Axiom regional edge domain to use for ingestion.
    ///
    /// Specify the domain name only (no scheme, no path).
    /// When set, data will be sent to `https://{region}/v1/ingest/{dataset}`.
    /// If `url` is also set, `url` takes precedence.
    #[configurable(metadata(docs::examples = "${AXIOM_REGION}"))]
    #[configurable(metadata(docs::examples = "mumbai.axiom.co"))]
    #[configurable(metadata(docs::examples = "eu-central-1.aws.edge.axiom.co"))]
    region: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    request: RequestConfig,

    /// The compression algorithm to use.
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
        Input::new(DataType::Metric | DataType::Log | DataType::Trace)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl AxiomConfig {
    fn build_endpoint(&self) -> String {
        // Priority: url > region > default cloud endpoint

        // If url is set, check if it has a path
        if let Some(url) = &self.url {
            let url = url.trim_end_matches('/');

            // Parse URL to check if path is provided
            // If path is empty or just "/", append the legacy format for backwards compatibility
            // Otherwise, use the URL as-is
            if let Ok(parsed) = url::Url::parse(url) {
                let path = parsed.path();
                if path.is_empty() || path == "/" {
                    // Backwards compatibility: append legacy path format
                    return format!("{}/v1/datasets/{}/ingest", url, self.dataset);
                }
            }

            // URL has a custom path, use as-is
            return url.to_string();
        }

        // If region is set, build the regional edge endpoint
        if let Some(region) = &self.region {
            let region = region.trim_end_matches('/');
            return format!("https://{}/v1/ingest/{}", region, self.dataset);
        }

        // Default: use cloud endpoint with legacy path format
        format!("{}/v1/datasets/{}/ingest", CLOUD_URL, self.dataset)
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::AxiomConfig>();
    }

    #[test]
    fn test_region_domain_only() {
        // region: mumbai.axiomdomain.co → https://mumbai.axiomdomain.co/v1/ingest/test-3
        let config = super::AxiomConfig {
            region: Some("mumbai.axiomdomain.co".to_string()),
            dataset: "test-3".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(
            endpoint,
            "https://mumbai.axiomdomain.co/v1/ingest/test-3"
        );
    }

    #[test]
    fn test_default_no_config() {
        // No url, no region → https://api.axiom.co/v1/datasets/foo/ingest
        let config = super::AxiomConfig {
            dataset: "foo".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(
            endpoint,
            "https://api.axiom.co/v1/datasets/foo/ingest"
        );
    }

    #[test]
    fn test_url_with_custom_path() {
        // url: http://localhost:3400/ingest → http://localhost:3400/ingest (as-is)
        let config = super::AxiomConfig {
            url: Some("http://localhost:3400/ingest".to_string()),
            dataset: "meh".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(
            endpoint,
            "http://localhost:3400/ingest"
        );
    }

    #[test]
    fn test_url_without_path_backwards_compat() {
        // url: https://api.eu.axiom.co/ → https://api.eu.axiom.co/v1/datasets/qoo/ingest
        let config = super::AxiomConfig {
            url: Some("https://api.eu.axiom.co".to_string()),
            dataset: "qoo".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(
            endpoint,
            "https://api.eu.axiom.co/v1/datasets/qoo/ingest"
        );

        // Also test with trailing slash
        let config = super::AxiomConfig {
            url: Some("https://api.eu.axiom.co/".to_string()),
            dataset: "qoo".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(
            endpoint,
            "https://api.eu.axiom.co/v1/datasets/qoo/ingest"
        );
    }

    #[test]
    fn test_url_takes_precedence_over_region() {
        // When both url and region are set, url takes precedence
        let config = super::AxiomConfig {
            url: Some("http://localhost:3400/ingest".to_string()),
            region: Some("mumbai.axiomdomain.co".to_string()),
            dataset: "test".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(
            endpoint,
            "http://localhost:3400/ingest"
        );
    }

    #[test]
    fn test_production_regional_edges() {
        // Production AWS edge
        let config = super::AxiomConfig {
            region: Some("eu-central-1.aws.edge.axiom.co".to_string()),
            dataset: "my-dataset".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(
            endpoint,
            "https://eu-central-1.aws.edge.axiom.co/v1/ingest/my-dataset"
        );
    }

    #[test]
    fn test_staging_environment_edges() {
        // Staging environment edge
        let config = super::AxiomConfig {
            region: Some("us-east-1.edge.staging.axiomdomain.co".to_string()),
            dataset: "test-dataset".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(
            endpoint,
            "https://us-east-1.edge.staging.axiomdomain.co/v1/ingest/test-dataset"
        );
    }

    #[test]
    fn test_dev_environment_edges() {
        // Dev environment edge
        let config = super::AxiomConfig {
            region: Some("eu-west-1.edge.dev.axiomdomain.co".to_string()),
            dataset: "dev-dataset".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(
            endpoint,
            "https://eu-west-1.edge.dev.axiomdomain.co/v1/ingest/dev-dataset"
        );
    }
}

#[cfg(feature = "axiom-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use std::env;

    use chrono::{DateTime, Duration, Utc};
    use futures::stream;
    use serde::{Deserialize, Serialize};
    use vector_lib::event::{BatchNotifier, BatchStatus, Event, LogEvent};

    use super::*;
    use crate::{
        config::SinkContext,
        sinks::axiom::AxiomConfig,
        test_util::components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
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
                "['{dataset}'] | where test_id == '{test_id}' | order by _time desc | limit 2"
            ),
            start_time: Utc::now() - Duration::minutes(10),
            end_time: Utc::now() + Duration::minutes(10),
        };
        let query_res: QueryResponse = client
            .post(format!("{url}/v1/datasets/_apl?format=legacy"))
            .header("X-Axiom-Org-Id", org_id)
            .header("Authorization", format!("Bearer {token}"))
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
