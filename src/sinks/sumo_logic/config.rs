//! Configuration for the `sumo_logic` sink.

use bytes::Bytes;
use http::{Request, Uri};
use vector_lib::codecs::{encoding::Framer, JsonSerializerConfig, NewlineDelimitedEncoderConfig};

use super::{request_builder::SumoLogicRequestBuilder, sink::SumoLogicSink};
use crate::{
    http::HttpClient,
    sinks::{
        prelude::*,
        util::{
            http::{http_response_retry_logic, HttpService, HttpServiceRequestBuilder},
            RealtimeSizeBasedDefaultBatchSettings,
        },
    },
};

/// Configuration for the `sumo_logic` sink.
#[configurable_component(sink("sumo_logic"))]
#[derive(Clone, Debug)]
pub struct SumoLogicConfig {
    /// The endpoint to send HTTP traffic to.
    ///
    /// This should include the protocol and host, but can also include the port, path, and any other valid part of a URI.
    #[configurable(metadata(
        docs::examples = "http://localhost:3000/",
        docs::examples = "http://example.com/endpoint/",
    ))]
    endpoint: String,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default)]
    request: TowerRequestConfig,

    #[configurable(derived)]
    tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(default)]
    batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,
}

impl GenerateConfig for SumoLogicConfig {
    fn generate_config() -> toml::Value {
        toml::from_str("").unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "sumo_logic")]
impl SinkConfig for SumoLogicConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batch_settings: BatcherSettings = self.batch.validate()?.into_batcher_settings()?;

        let healthcheck = healthcheck().boxed();

        let tls = TlsSettings::from_options(&self.tls).unwrap();
        let client = HttpClient::new(tls, cx.proxy())?;

        //TODO: Expose encoding configuration instead of hard coding JSON & default transformer
        let transformer: Transformer = Default::default();
        let encoder = (
            transformer,
            Encoder::<Framer>::new(
                NewlineDelimitedEncoderConfig.build().into(),
                JsonSerializerConfig::default().build().into(),
            ),
        );

        let request_builder = SumoLogicRequestBuilder { encoder };

        //TODO: Expose endpoint name and secret key seperately
        let uri: Uri = self.endpoint.clone().parse()?;

        let sumo_logic_service_request_builder = SumoLogicServiceRequestBuilder { uri };

        let service: HttpService<SumoLogicServiceRequestBuilder> =
            HttpService::new(client, sumo_logic_service_request_builder);
        let request_limits = self.request.into_settings();

        let service = ServiceBuilder::new()
            .settings(request_limits, http_response_retry_logic())
            .service(service);

        let sink = VectorSink::from_event_streamsink(SumoLogicSink::new(
            service,
            batch_settings,
            request_builder,
        ));

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Debug, Clone)]
pub(super) struct SumoLogicServiceRequestBuilder {
    pub(super) uri: Uri,
}

impl HttpServiceRequestBuilder for SumoLogicServiceRequestBuilder {
    fn build(&self, body: Bytes) -> Request<Bytes> {
        let request: Request<Bytes> = Request::post(self.uri.clone())
            .header("Content-Type", "application/json")
            .body(body)
            .unwrap();

        request
    }
}

//TODO: Configure healthcheck endpoint if one exists
async fn healthcheck() -> crate::Result<()> {
    Ok(())
}
