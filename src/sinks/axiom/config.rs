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

/// Configuration of the URL/region to use when interacting with Axiom.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(default)]
pub struct UrlOrRegion {
    /// URI of the Axiom endpoint to send data to.
    ///
    /// If a path is provided, the URL is used as-is.
    /// If no path (or only `/`) is provided, `/v1/datasets/{dataset}/ingest` is appended for backwards compatibility.
    /// This takes precedence over `region` if both are set (but both should not be set).
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "https://api.eu.axiom.co"))]
    #[configurable(metadata(docs::examples = "http://localhost:3400/ingest"))]
    #[configurable(metadata(docs::examples = "${AXIOM_URL}"))]
    pub url: Option<String>,

    /// The Axiom regional edge domain to use for ingestion.
    ///
    /// Specify the domain name only (no scheme, no path).
    /// When set, data is sent to `https://{region}/v1/ingest/{dataset}`.
    /// Cannot be used together with `url`.
    #[configurable(metadata(docs::examples = "${AXIOM_REGION}"))]
    #[configurable(metadata(docs::examples = "mumbai.axiom.co"))]
    #[configurable(metadata(docs::examples = "eu-central-1.aws.edge.axiom.co"))]
    pub region: Option<String>,
}

impl UrlOrRegion {
    /// Validates that url and region are not both set.
    fn validate(&self) -> crate::Result<()> {
        if self.url.is_some() && self.region.is_some() {
            return Err("Cannot set both `url` and `region`. Please use only one.".into());
        }
        Ok(())
    }

    /// Returns the url if set.
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// Returns the region if set.
    pub fn region(&self) -> Option<&str> {
        self.region.as_deref()
    }
}

/// Configuration for the `axiom` sink.
#[configurable_component(sink("axiom", "Deliver log events to Axiom."))]
#[derive(Clone, Debug, Default)]
pub struct AxiomConfig {
    /// The Axiom organization ID.
    ///
    /// Only required when using personal tokens.
    #[configurable(metadata(docs::examples = "${AXIOM_ORG_ID}"))]
    #[configurable(metadata(docs::examples = "123abc"))]
    pub org_id: Option<String>,

    /// The Axiom API token.
    #[configurable(metadata(docs::examples = "${AXIOM_TOKEN}"))]
    #[configurable(metadata(docs::examples = "123abc"))]
    pub token: SensitiveString,

    /// The Axiom dataset to write to.
    #[configurable(metadata(docs::examples = "${AXIOM_DATASET}"))]
    #[configurable(metadata(docs::examples = "vector_rocks"))]
    pub dataset: String,

    /// Configuration for the URL or regional edge endpoint.
    #[serde(flatten)]
    #[configurable(derived)]
    pub endpoint: UrlOrRegion,

    #[configurable(derived)]
    #[serde(default)]
    pub request: RequestConfig,

    /// The compression algorithm to use.
    #[configurable(derived)]
    #[serde(default = "Compression::zstd_default")]
    pub compression: Compression,

    /// The TLS settings for the connection.
    ///
    /// Optional, constrains TLS settings for this sink.
    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

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
    pub acknowledgements: AcknowledgementsConfig,
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
        // Validate that url and region are not both set
        self.endpoint.validate()?;

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
        if let Some(url) = self.endpoint.url() {
            let url = url.trim_end_matches('/');

            // Parse URL to check if path is provided
            // If path is empty or just "/", append the legacy format for backwards compatibility
            // Otherwise, use the URL as-is
            if let Ok(parsed) = url::Url::parse(url) {
                let path = parsed.path();
                if path.is_empty() || path == "/" {
                    // Backwards compatibility: append legacy path format
                    return format!("{url}/v1/datasets/{}/ingest", self.dataset);
                }
            }

            // URL has a custom path, use as-is
            return url.to_string();
        }

        // If region is set, build the regional edge endpoint
        if let Some(region) = self.endpoint.region() {
            let region = region.trim_end_matches('/');
            return format!("https://{region}/v1/ingest/{}", self.dataset);
        }

        // Default: use cloud endpoint with legacy path format
        format!("{CLOUD_URL}/v1/datasets/{}/ingest", self.dataset)
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
            endpoint: super::UrlOrRegion {
                region: Some("mumbai.axiomdomain.co".to_string()),
                url: None,
            },
            dataset: "test-3".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(endpoint, "https://mumbai.axiomdomain.co/v1/ingest/test-3");
    }

    #[test]
    fn test_default_no_config() {
        // No url, no region → https://api.axiom.co/v1/datasets/foo/ingest
        let config = super::AxiomConfig {
            dataset: "foo".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(endpoint, "https://api.axiom.co/v1/datasets/foo/ingest");
    }

    #[test]
    fn test_url_with_custom_path() {
        // url: http://localhost:3400/ingest → http://localhost:3400/ingest (as-is)
        let config = super::AxiomConfig {
            endpoint: super::UrlOrRegion {
                url: Some("http://localhost:3400/ingest".to_string()),
                region: None,
            },
            dataset: "meh".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(endpoint, "http://localhost:3400/ingest");
    }

    #[test]
    fn test_url_without_path_backwards_compat() {
        // url: https://api.eu.axiom.co/ → https://api.eu.axiom.co/v1/datasets/qoo/ingest
        let config = super::AxiomConfig {
            endpoint: super::UrlOrRegion {
                url: Some("https://api.eu.axiom.co".to_string()),
                region: None,
            },
            dataset: "qoo".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(endpoint, "https://api.eu.axiom.co/v1/datasets/qoo/ingest");

        // Also test with trailing slash
        let config = super::AxiomConfig {
            endpoint: super::UrlOrRegion {
                url: Some("https://api.eu.axiom.co/".to_string()),
                region: None,
            },
            dataset: "qoo".to_string(),
            ..Default::default()
        };
        let endpoint = config.build_endpoint();
        assert_eq!(endpoint, "https://api.eu.axiom.co/v1/datasets/qoo/ingest");
    }

    #[test]
    fn test_both_url_and_region_fails_validation() {
        // When both url and region are set, validation should fail
        let endpoint = super::UrlOrRegion {
            url: Some("http://localhost:3400/ingest".to_string()),
            region: Some("mumbai.axiomdomain.co".to_string()),
        };

        let result = endpoint.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Cannot set both `url` and `region`. Please use only one."
        );
    }

    #[test]
    fn test_url_or_region_deserialization_with_url() {
        // Test that url can be deserialized at the top level (flattened)
        let config: super::AxiomConfig = toml::from_str(
            r#"
            token = "test-token"
            dataset = "test-dataset"
            url = "https://api.eu.axiom.co"
            "#,
        )
        .unwrap();

        assert_eq!(config.endpoint.url(), Some("https://api.eu.axiom.co"));
        assert_eq!(config.endpoint.region(), None);
    }

    #[test]
    fn test_url_or_region_deserialization_with_region() {
        // Test that region can be deserialized at the top level (flattened)
        let config: super::AxiomConfig = toml::from_str(
            r#"
            token = "test-token"
            dataset = "test-dataset"
            region = "mumbai.axiom.co"
            "#,
        )
        .unwrap();

        assert_eq!(config.endpoint.url(), None);
        assert_eq!(config.endpoint.region(), Some("mumbai.axiom.co"));
    }

    #[test]
    fn test_production_regional_edges() {
        // Production AWS edge
        let config = super::AxiomConfig {
            endpoint: super::UrlOrRegion {
                region: Some("eu-central-1.aws.edge.axiom.co".to_string()),
                url: None,
            },
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
            endpoint: super::UrlOrRegion {
                region: Some("us-east-1.edge.staging.axiomdomain.co".to_string()),
                url: None,
            },
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
            endpoint: super::UrlOrRegion {
                region: Some("eu-west-1.edge.dev.axiomdomain.co".to_string()),
                url: None,
            },
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
