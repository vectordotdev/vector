use std::{sync::Arc, time::Instant};

use iggy::prelude::{Client, IggyClient, IggyProducer};
use snafu::ResultExt;
use tokio::sync::OnceCell;
use vector_lib::codecs::JsonSerializerConfig;

use super::{ConnectSnafu, IggyError, sink::IggySink};
use crate::common::iggy::{
    RESOLVE_TIMEOUT, login_if_http, retained_operation, secure_connection_string,
};
use crate::config::ValidatedSink;
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

    pub encoding: EncodingConfig,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[serde(default)]
    pub request: TowerRequestConfig<IggyTowerRequestConfigDefaults>,

    #[serde(default)]
    pub batch: BatchConfig<IggyDefaultBatchSettings>,
}

const fn default_partitions() -> u32 {
    1
}

impl GenerateConfig for IggySinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            url: "iggy+tcp://iggy:iggy@127.0.0.1:8090".into(),
            stream: "vector".into(),
            topic: "logs".into(),
            partitions: default_partitions(),
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: AcknowledgementsConfig::default(),
            request: TowerRequestConfig::default(),
            batch: BatchConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "iggy")]
impl SinkConfig for IggySinkConfig {
    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

/// How many retained operations the sink replays before it reports the failed
/// producer initialization, so a lane that stays blocked cannot spin forever.
const MAX_INIT_RECOVERIES: u32 = 3;

/// Broker client and producer, created on first use. Topology construction
/// must not touch the network, so `healthcheck` and the first send initialize
/// the pair on demand.
pub(super) struct Connection {
    url: String,
    stream: String,
    topic: String,
    partitions: u32,
    client: OnceCell<Arc<IggyClient>>,
    producer: OnceCell<Arc<IggyProducer>>,
}

impl Connection {
    pub(super) fn new(config: &IggySinkConfig) -> Self {
        Self {
            url: config.url.clone(),
            stream: config.stream.clone(),
            topic: config.topic.clone(),
            partitions: config.partitions,
            client: OnceCell::new(),
            producer: OnceCell::new(),
        }
    }

    pub(super) fn url(&self) -> &str {
        &self.url
    }

    pub(super) fn connected_client(&self) -> Option<&Arc<IggyClient>> {
        self.client.get()
    }

    /// Connect and authenticate only; the healthcheck needs no producer and
    /// must not create the stream or topic as a side effect.
    pub(super) async fn client(&self) -> Result<Arc<IggyClient>, IggyError> {
        use iggy::prelude::{Client, IggyClientBuilder};

        self.client
            .get_or_try_init(|| async {
                let client =
                    IggyClientBuilder::from_connection_string(&secure_connection_string(&self.url))
                        .context(ConnectSnafu)?
                        .build()
                        .context(ConnectSnafu)?;

                client.connect().await.context(ConnectSnafu)?;
                login_if_http(&client, &self.url)
                    .await
                    .context(ConnectSnafu)?;
                Ok::<_, IggyError>(Arc::new(client))
            })
            .await
            .map(Arc::clone)
    }

    pub(super) async fn get(&self) -> Result<(Arc<IggyClient>, Arc<IggyProducer>), IggyError> {
        use iggy::prelude::{IggyExpiry, MaxTopicSize};

        let client = self.client().await?;

        let producer = self
            .producer
            .get_or_try_init(|| async {
                let producer = client
                    .producer(&self.stream, &self.topic)
                    .context(ConnectSnafu)?
                    .create_stream_if_not_exists()
                    .create_topic_if_not_exists(
                        self.partitions,
                        IggyExpiry::ServerDefault,
                        MaxTopicSize::ServerDefault,
                    )
                    .build();

                let mut recovered = 0;
                loop {
                    match producer.init().await {
                        Ok(()) => return Ok(Arc::new(producer)),
                        Err(error) => {
                            let Some(operation) = retained_operation(&error) else {
                                return Err(IggyError::Connect { source: error });
                            };
                            if recovered == MAX_INIT_RECOVERIES {
                                return Err(IggyError::Connect { source: error });
                            }
                            recovered += 1;
                            // A timed-out stream or topic creation stays
                            // retained in the client's metadata lane, and init
                            // runs before any send reaches the send-path
                            // recovery, so every later init would fail with
                            // `LaneBlocked` until the operation is replayed.
                            match client
                                .resolve_operation(operation, Instant::now() + RESOLVE_TIMEOUT)
                                .await
                            {
                                Ok(()) | Err(iggy::prelude::IggyError::OperationNotRetained(_)) => {
                                }
                                Err(error) => return Err(IggyError::Connect { source: error }),
                            }
                        }
                    }
                }
            })
            .await?;

        Ok((client, Arc::clone(producer)))
    }
}

async fn healthcheck(connection: Arc<Connection>) -> crate::Result<()> {
    use iggy::prelude::SystemClient;
    let client = connection.client().await?;
    // A missing stream or topic is created on first send. Distinguishing that
    // from a broker that denies creation needs the create attempt itself, so
    // the health check stays a ping; a denied create surfaces on the first event.
    client.ping().await.map_err(Into::into)
}

/// Retained state produced by structural validation.
#[derive(Debug)]
pub struct ValidatedIggySink;

#[async_trait::async_trait]
impl ValidatedSink for IggySinkConfig {
    type Validated = ValidatedIggySink;

    fn validate(&self) -> crate::Result<ValidatedIggySink> {
        Ok(ValidatedIggySink)
    }

    async fn build(
        &self,
        _validated: &ValidatedIggySink,
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        if self.partitions == 0 {
            return Err("`partitions` must be at least 1".into());
        }
        // Connecting here would make an unreachable broker a topology error
        // before healthcheck settings apply, defeating `require_healthy =
        // false` and `vector validate --skip-healthchecks`.
        let connection = Arc::new(Connection::new(self));
        let healthcheck = healthcheck(Arc::clone(&connection)).boxed();
        let sink = IggySink::new(self, connection)?;
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}
