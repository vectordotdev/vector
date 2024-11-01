use bytes::Bytes;
use chrono::Utc;
use futures::StreamExt;
use snafu::{ResultExt, Snafu};
use tokio_util::codec::FramedRead;
use vector_lib::codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol, Registered,
};
use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path, path, OwnedValuePath};
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    EstimatedJsonEncodedSizeOf,
};
use vrl::value::Kind;

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{log_schema, GenerateConfig, SourceConfig, SourceContext, SourceOutput},
    event::Event,
    internal_events::{EventsReceived, StreamClosedError},
    serde::{default_decoding, default_framing_message_based},
};

mod channel;
mod list;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to build redis client: {}", source))]
    Client { source: redis::RedisError },
}

/// Data type to use for reading messages from Redis.
#[configurable_component]
#[derive(Copy, Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum DataTypeConfig {
    /// The `list` data type.
    #[derivative(Default)]
    List,

    /// The `channel` data type.
    ///
    /// This is based on Redis' Pub/Sub capabilities.
    Channel,
}

/// Options for the Redis `list` data type.
#[configurable_component]
#[derive(Copy, Clone, Debug, Default, Derivative, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub struct ListOption {
    #[configurable(derived)]
    method: Method,
}

/// Method for getting events from the `list` data type.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    /// Pop messages from the head of the list.
    #[derivative(Default)]
    Lpop,

    /// Pop messages from the tail of the list.
    Rpop,
}

pub struct ConnectionInfo {
    protocol: &'static str,
    endpoint: String,
}

impl From<&redis::ConnectionInfo> for ConnectionInfo {
    fn from(redis_conn_info: &redis::ConnectionInfo) -> Self {
        let (protocol, endpoint) = match &redis_conn_info.addr {
            redis::ConnectionAddr::Tcp(host, port)
            | redis::ConnectionAddr::TcpTls { host, port, .. } => {
                ("tcp", format!("{}:{}", host, port))
            }
            redis::ConnectionAddr::Unix(path) => ("uds", path.to_string_lossy().to_string()),
        };

        Self { protocol, endpoint }
    }
}

/// Configuration for the `redis` source.
#[configurable_component(source("redis", "Collect observability data from Redis."))]
#[derive(Clone, Debug, Derivative)]
#[serde(deny_unknown_fields)]
pub struct RedisSourceConfig {
    /// The Redis data type (`list` or `channel`) to use.
    #[serde(default)]
    data_type: DataTypeConfig,

    #[configurable(derived)]
    list: Option<ListOption>,

    /// The Redis URL to connect to.
    ///
    /// The URL must take the form of `protocol://server:port/db` where the `protocol` can either be `redis` or `rediss` for connections secured using TLS.
    #[configurable(metadata(docs::examples = "redis://127.0.0.1:6379/0"))]
    url: String,

    /// The Redis key to read messages from.
    #[configurable(metadata(docs::examples = "vector"))]
    key: String,

    /// Sets the name of the log field to use to add the key to each event.
    ///
    /// The value is the Redis key that the event was read from.
    ///
    /// By default, this is not set and the field is not automatically added.
    #[configurable(metadata(docs::examples = "redis_key"))]
    redis_key: Option<OptionalValuePath>,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    decoding: DeserializerConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,
}

impl GenerateConfig for RedisSourceConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            url = "redis://127.0.0.1:6379/0"
            key = "vector"
            data_type = "list"
            list.method = "lpop"
            redis_key = "redis_key"
            "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "redis")]
impl SourceConfig for RedisSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        // A key must be specified to actually query i.e. the list to pop from, or the channel to subscribe to.
        if self.key.is_empty() {
            return Err("`key` cannot be empty.".into());
        }
        let redis_key = self.redis_key.clone().and_then(|k| k.path);

        let client = redis::Client::open(self.url.as_str()).context(ClientSnafu {})?;
        let connection_info = ConnectionInfo::from(client.get_connection_info());
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let bytes_received = register!(BytesReceived::from(Protocol::from(
            connection_info.protocol
        )));
        let events_received = register!(EventsReceived);
        let handler = InputHandler {
            client,
            bytes_received: bytes_received.clone(),
            events_received: events_received.clone(),
            key: self.key.clone(),
            redis_key,
            decoder,
            cx,
            log_namespace,
        };

        match self.data_type {
            DataTypeConfig::List => {
                let method = self.list.unwrap_or_default().method;
                handler.watch(method).await
            }
            DataTypeConfig::Channel => handler.subscribe(connection_info).await,
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let redis_key_path = self
            .redis_key
            .clone()
            .and_then(|k| k.path)
            .map(LegacyKey::InsertIfEmpty);

        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_source_metadata(
                Self::NAME,
                redis_key_path,
                &owned_value_path!("key"),
                Kind::bytes(),
                None,
            )
            .with_standard_vector_source_metadata();

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

struct InputHandler {
    pub client: redis::Client,
    pub bytes_received: Registered<BytesReceived>,
    pub events_received: Registered<EventsReceived>,
    pub key: String,
    pub redis_key: Option<OwnedValuePath>,
    pub decoder: Decoder,
    pub log_namespace: LogNamespace,
    pub cx: SourceContext,
}

impl InputHandler {
    async fn handle_line(&mut self, line: String) -> Result<(), ()> {
        let now = Utc::now();

        self.bytes_received.emit(ByteSize(line.len()));

        let mut stream = FramedRead::new(line.as_ref(), self.decoder.clone());
        while let Some(next) = stream.next().await {
            match next {
                Ok((events, _byte_size)) => {
                    let count = events.len();
                    let byte_size = events.estimated_json_encoded_size_of();
                    self.events_received.emit(CountByteSize(count, byte_size));

                    let events = events.into_iter().map(|mut event| {
                        if let Event::Log(ref mut log) = event {
                            self.log_namespace.insert_vector_metadata(
                                log,
                                log_schema().source_type_key(),
                                path!("source_type"),
                                Bytes::from(RedisSourceConfig::NAME),
                            );
                            self.log_namespace.insert_vector_metadata(
                                log,
                                log_schema().timestamp_key(),
                                path!("ingest_timestamp"),
                                now,
                            );

                            self.log_namespace.insert_source_metadata(
                                RedisSourceConfig::NAME,
                                log,
                                self.redis_key.as_ref().map(LegacyKey::InsertIfEmpty),
                                path!("key"),
                                self.key.as_str(),
                            );
                        };

                        event
                    });

                    if (self.cx.out.send_batch(events).await).is_err() {
                        emit!(StreamClosedError { count });
                        return Err(());
                    }
                }
                Err(error) => {
                    // Error is logged by `crate::codecs::Decoder`, no further
                    // handling is needed here.
                    if !error.can_continue() {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RedisSourceConfig>();
    }
}

#[cfg(all(test, feature = "redis-integration-tests"))]
mod integration_test {
    use redis::AsyncCommands;

    use super::*;
    use crate::{
        config::log_schema,
        test_util::{
            collect_n,
            components::{run_and_assert_source_compliance_n, SOURCE_TAGS},
            random_string,
        },
        SourceSender,
    };
    use vrl::value;

    const REDIS_SERVER: &str = "redis://redis:6379/0";

    #[tokio::test]
    async fn redis_source_list_rpop() {
        // Push some test data into a list object which we'll read from.
        let client = redis::Client::open(REDIS_SERVER).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();

        let key = format!("test-key-{}", random_string(10));
        debug!("Test key name: {}.", key);

        let _: i32 = conn.rpush(&key, "1").await.unwrap();
        let _: i32 = conn.rpush(&key, "2").await.unwrap();
        let _: i32 = conn.rpush(&key, "3").await.unwrap();

        // Now run the source and make sure we get all three events.
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::List,
            list: Some(ListOption {
                method: Method::Rpop,
            }),
            url: REDIS_SERVER.to_owned(),
            key: key.clone(),
            redis_key: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            log_namespace: Some(false),
        };

        let events = run_and_assert_source_compliance_n(config, 3, &SOURCE_TAGS).await;

        assert_eq!(
            events[0].as_log()[log_schema().message_key().unwrap().to_string()],
            "3".into()
        );
        assert_eq!(
            events[1].as_log()[log_schema().message_key().unwrap().to_string()],
            "2".into()
        );
        assert_eq!(
            events[2].as_log()[log_schema().message_key().unwrap().to_string()],
            "1".into()
        );
    }

    #[tokio::test]
    async fn redis_source_list_rpop_with_log_namespace() {
        // Push some test data into a list object which we'll read from.
        let client = redis::Client::open(REDIS_SERVER).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();

        let key = format!("test-key-{}", random_string(10));
        debug!("Test key name: {}.", key);

        let _: i32 = conn.rpush(&key, "1").await.unwrap();

        // Now run the source and make sure we get all three events.
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::List,
            list: Some(ListOption {
                method: Method::Rpop,
            }),
            url: REDIS_SERVER.to_owned(),
            key: key.clone(),
            redis_key: Some(OptionalValuePath::from(owned_value_path!("remapped_key"))),
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            log_namespace: Some(true),
        };

        let events = run_and_assert_source_compliance_n(config, 1, &SOURCE_TAGS).await;

        let log_event = events[0].as_log();
        let meta = log_event.metadata();

        assert_eq!(log_event.value(), &"1".into());
        assert_eq!(
            meta.value()
                .get(path!(RedisSourceConfig::NAME, "key"))
                .unwrap(),
            &value!(key)
        );
    }

    #[tokio::test]
    async fn redis_source_list_lpop() {
        // Push some test data into a list object which we'll read from.
        let client = redis::Client::open(REDIS_SERVER).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();

        let key = format!("test-key-{}", random_string(10));
        debug!("Test key name: {}.", key);

        let _: i32 = conn.rpush(&key, "1").await.unwrap();
        let _: i32 = conn.rpush(&key, "2").await.unwrap();
        let _: i32 = conn.rpush(&key, "3").await.unwrap();

        // Now run the source and make sure we get all three events.
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::List,
            list: Some(ListOption {
                method: Method::Lpop,
            }),
            url: REDIS_SERVER.to_owned(),
            key: key.clone(),
            redis_key: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            log_namespace: Some(false),
        };

        let events = run_and_assert_source_compliance_n(config, 3, &SOURCE_TAGS).await;

        assert_eq!(
            events[0].as_log()[log_schema().message_key().unwrap().to_string()],
            "1".into()
        );
        assert_eq!(
            events[1].as_log()[log_schema().message_key().unwrap().to_string()],
            "2".into()
        );
        assert_eq!(
            events[2].as_log()[log_schema().message_key().unwrap().to_string()],
            "3".into()
        );
    }

    #[tokio::test]
    async fn redis_source_channel_consume_event() {
        let key = format!("test-channel-{}", random_string(10));
        let text = "test message for channel";

        // Create the source and spawn it in the background, so that we're already listening before we publish any messages.
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::Channel,
            list: None,
            url: REDIS_SERVER.to_owned(),
            key: key.clone(),
            redis_key: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            log_namespace: Some(false),
        };

        let (tx, rx) = SourceSender::new_test();
        let context = SourceContext::new_test(tx, None);
        let source = config
            .build(context)
            .await
            .expect("source should not fail to build");

        tokio::spawn(source);

        // Briefly wait to ensure the source is subscribed.
        //
        // TODO: This is a prime example of where being able to check if the shutdown signal had been polled at least
        // once would serve as the most precise indicator of "is the source ready and waiting to receive?".
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Now create a normal Redis client and use it to publish a bunch of message, which we'll ensure the source consumes.
        let client = redis::Client::open(REDIS_SERVER).unwrap();

        let mut async_conn = client
            .get_async_connection()
            .await
            .expect("Failed to get redis async connection.");

        for _i in 0..10000 {
            let _: i32 = async_conn.publish(key.clone(), text).await.unwrap();
        }

        let events = collect_n(rx, 10000).await;
        assert_eq!(events.len(), 10000);

        for event in events {
            assert_eq!(
                event.as_log()[log_schema().message_key().unwrap().to_string()],
                text.into()
            );
            assert_eq!(
                event.as_log()[log_schema().source_type_key().unwrap().to_string()],
                RedisSourceConfig::NAME.into()
            );
        }
    }
}
