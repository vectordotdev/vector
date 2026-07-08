use std::sync::Arc;

use iggy::prelude::{IggyClient, IggyProducer};
use snafu::ResultExt;
use vector_lib::codecs::JsonSerializerConfig;

use super::{ConnectSnafu, IggyError, sink::IggySink};
use crate::sinks::{prelude::*, util::service::TowerRequestConfigDefaults};

#[derive(Clone, Copy, Debug)]
pub struct IggyTowerRequestConfigDefaults;

impl TowerRequestConfigDefaults for IggyTowerRequestConfigDefaults {
    const CONCURRENCY: Concurrency = Concurrency::None;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct IggyDefaultBatchSettings;

impl SinkBatchSettings for IggyDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1000);
    const MAX_BYTES: Option<usize> = Some(1_000_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `iggy` sink.
#[configurable_component(sink(
    "iggy",
    "Publish observability data to a topic on the Iggy message streaming platform."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct IggySinkConfig {
    /// The Iggy [connection string][iggy_conn] of the server to publish to.
    ///
    /// The connection string takes the form
    /// `iggy+<protocol>://<credentials>@<host>:<port>` where `<protocol>` is one
    /// of `tcp`, `quic`, `http`, or `ws`, and `<credentials>` is either
    /// `username:password` or a personal access token. The legacy
    /// `iggy://user:pass@host:port` form (TCP, no explicit protocol) is also
    /// accepted.
    ///
    /// [iggy_conn]: https://iggy.apache.org/docs/connection-string
    #[configurable(metadata(docs::examples = "iggy+tcp://iggy:iggy@127.0.0.1:8090"))]
    #[configurable(metadata(docs::examples = "iggy+tcp://iggypat-1234567890abcdef@host:8090"))]
    pub url: String,

    /// The Iggy stream name to publish messages to. Created on connect if it does
    /// not already exist.
    #[configurable(metadata(docs::examples = "vector"))]
    pub stream: String,

    /// The Iggy topic name within the stream. Created on connect if it does not
    /// already exist.
    #[configurable(metadata(docs::examples = "logs"))]
    pub topic: String,

    /// Number of partitions to create when the topic is created on connect. Has
    /// no effect if the topic already exists.
    #[serde(default = "default_partitions")]
    #[configurable(metadata(docs::examples = 1_u32))]
    pub partitions: u32,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig<IggyTowerRequestConfigDefaults>,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<IggyDefaultBatchSettings>,
}

const fn default_partitions() -> u32 {
    1
}

impl GenerateConfig for IggySinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            url: "iggy+tcp://iggy:iggy@127.0.0.1:8090".into(),
            stream: "vector".into(),
            topic: "logs".into(),
            partitions: default_partitions(),
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: Default::default(),
            request: Default::default(),
            batch: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "iggy")]
impl SinkConfig for IggySinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        if self.partitions == 0 {
            return Err("`partitions` must be at least 1".into());
        }
        let (client, producer) = self.connect_and_init().await?;
        let healthcheck = healthcheck(Arc::clone(&client)).boxed();
        let sink = IggySink::new(self.clone(), Arc::clone(&client), producer)?;
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl IggySinkConfig {
    pub(super) async fn connect_and_init(
        &self,
    ) -> Result<(Arc<IggyClient>, Arc<IggyProducer>), IggyError> {
        use iggy::prelude::{IggyClientBuilder, MaxTopicSize};

        let client = IggyClientBuilder::from_connection_string(&self.url)
            .context(ConnectSnafu)?
            .build()
            .context(ConnectSnafu)?;

        use iggy::prelude::Client;
        client.connect().await.context(ConnectSnafu)?;

        let producer = client
            .producer(&self.stream, &self.topic)
            .context(ConnectSnafu)?
            .create_stream_if_not_exists()
            .create_topic_if_not_exists(
                self.partitions,
                None,
                iggy::prelude::IggyExpiry::ServerDefault,
                MaxTopicSize::ServerDefault,
            )
            // Disable SDK-level send retries so the Tower retry layer is the
            // single policy for the sink (avoids double-retry amplification).
            .send_retries(None, None)
            .build();

        producer.init().await.context(ConnectSnafu)?;

        Ok((Arc::new(client), Arc::new(producer)))
    }
}

async fn healthcheck(client: Arc<IggyClient>) -> crate::Result<()> {
    use iggy::prelude::SystemClient;
    client.ping().await.map_err(Into::into)
}
