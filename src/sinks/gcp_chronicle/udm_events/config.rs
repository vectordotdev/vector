use vector_config::component::GenerateConfig;
use vector_lib::{
    config::{AcknowledgementsConfig, Input}, schema, sink::VectorSink, tls::TlsSettings
};

use indoc::indoc;
use vector_lib::configurable::configurable_component;

use crate::{
    config::{SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        gcp_chronicle::{
            config::ChronicleCommonConfig, service::build_healthcheck
        },
        Healthcheck,
    },
};
use goauth::scopes::Scope;
use vrl::value::Kind;

/// Configuration for the `gcp_chronicle_udm_events` sink.
#[configurable_component(sink(
    "gcp_chronicle_udm_events",
    "Batch create UDM log events in Google Chronicle."
))]
#[derive(Clone, Debug)]
pub struct ChronicleUDMEventsConfig {
    #[serde(flatten)]
    pub chronicle_common: ChronicleCommonConfig,
}

impl GenerateConfig for ChronicleUDMEventsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            credentials_path = "/path/to/credentials.json"
            customer_id = "customer_id"
            compression = "gzip"
            encoding.codec = "json"
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_chronicle_udm_events")]
impl SinkConfig for ChronicleUDMEventsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let creds = self.chronicle_common.auth.build(Scope::MalachiteIngestion).await?;

        let tls = TlsSettings::from_options(&self.chronicle_common.tls)?;
        let client = HttpClient::new(tls, cx.proxy())?;

        let endpoint = self.chronicle_common.create_endpoint("v2/udmevents:batchCreate")?;

        // For the healthcheck we see if we can fetch the list of available log types.
        let healthcheck_endpoint = self.chronicle_common.create_endpoint("v2/logtypes")?;

        let healthcheck = build_healthcheck(client.clone(), &healthcheck_endpoint, creds.clone())?;
        creds.spawn_regenerate_token();
        let sink = self.build_sink(client, endpoint, creds)?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        let requirement =
            schema::Requirement::empty().required_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.chronicle_common.acknowledgements
    }
}
