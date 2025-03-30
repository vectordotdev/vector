//! `X-Ray` sink.
//! A sink that will send it's output to AWS X-Ray

use crate::{
    aws::{create_client, AwsAuthentication, ClientBuilder, RegionOrEndpoint},
    sinks::prelude::*,
};
use aws_sdk_xray::Client;
use aws_types::SdkConfig;
use vector_lib::internal_event::{
    ByteSize, BytesSent, EventsSent, InternalEventHandle, Output, Protocol,
};

#[configurable_component(sink("xray"))]
#[derive(Clone, Debug)]
/// A sink that will send it's output to AWS X-Ray
pub struct XRayConfig {
    #[serde(flatten)]
    #[configurable(derived)]
    pub region: RegionOrEndpoint,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub auth: AwsAuthentication,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for XRayConfig {
    fn generate_config() -> toml::Value {
        toml::from_str("").unwrap()
    }
}

pub type XRayClient = Client;

pub struct XRayClientBuilder;

impl ClientBuilder for XRayClientBuilder {
    type Client = XRayClient;

    fn build(&self, config: &SdkConfig) -> Self::Client {
        Client::new(config)
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "xray")]
impl SinkConfig for XRayConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = create_client::<XRayClientBuilder>(
            &XRayClientBuilder {},
            &self.auth,
            self.region.region(),
            self.region.endpoint(),
            cx.proxy(),
            self.tls.as_ref(),
            None,
        )
        .await?;
        let sink = VectorSink::from_event_streamsink(XRaySink { client });

        let healthcheck = Box::pin(async move { Ok(()) });

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
struct XRaySink {
    client: Client,
}

#[async_trait::async_trait]
impl StreamSink<Event> for XRaySink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

impl XRaySink {
    async fn run_inner(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let bytes_sent = register!(BytesSent::from(Protocol::NONE));
        let events_sent = register!(EventsSent::from(Output(None)));

        while let Some(mut event) = input.next().await {
            let bytes = serde_json::to_string(&event.as_log().value()).unwrap();
            self.client
                .put_trace_segments()
                .trace_segment_documents(bytes.clone())
                .send()
                .await
                .unwrap();
            bytes_sent.emit(ByteSize(bytes.len()));

            let event_byte_size = event.estimated_json_encoded_size_of();
            events_sent.emit(CountByteSize(1, event_byte_size));

            let finalizers = event.take_finalizers();
            finalizers.update_status(EventStatus::Delivered);
        }

        Ok(())
    }
}
