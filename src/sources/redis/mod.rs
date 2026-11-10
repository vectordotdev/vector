use chrono::Utc;
use futures::StreamExt;
use snafu::{ResultExt, Snafu};
use vector_lib::internal_event::{emit, register};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{
        Decoder, DecoderFramedRead, DecodingConfig, StreamDecodingError,
        decoding::{DeserializerConfig, FramingConfig},
    },
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol, Registered,
    },
    lookup::{OwnedValuePath, lookup_v2::OptionalValuePath, owned_value_path, path},
};
use vrl::value::{Kind, Value};

use crate::{
    config::{GenerateConfig, SourceConfig, SourceContext, SourceOutput, log_schema},
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
#[derive(Copy, Clone, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum DataTypeConfig {
    /// The `list` data type.
    #[default]
    List,

    /// The `channel` data type.
    ///
    /// This is based on Redis' Pub/Sub capabilities.
    Channel,

    /// The `pchannel` data type.
    ///
    /// Pattern-based Pub/Sub subscription using Redis `PSUBSCRIBE`. The `key` is interpreted
    /// as a glob-style channel pattern (for example, `news.*`).
    Pchannel,
}

/// Options for the Redis `list` data type.
#[configurable_component]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub struct ListOption {
    method: Method,
}

/// Method for getting events from the `list` data type.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    /// Pop messages from the head of the list.
    #[default]
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
            | redis::ConnectionAddr::TcpTls { host, port, .. } => ("tcp", format!("{host}:{port}")),
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
    /// The Redis data type (`list`, `channel`, or `pchannel`) to use.
    #[serde(default)]
    data_type: DataTypeConfig,

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

    /// Sets the name of the log field to use to add the matched channel to each event.
    ///
    /// The value is the concrete Redis channel that matched the subscribed pattern.
    /// Only applies to the `pchannel` data type.
    ///
    /// By default, this is not set and the field is not automatically added.
    #[configurable(metadata(docs::examples = "redis_channel"))]
    redis_channel: Option<OptionalValuePath>,

    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    framing: FramingConfig,

    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    decoding: DeserializerConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,
}

impl GenerateConfig for RedisSourceConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r#"
            url: "redis://127.0.0.1:6379/0"
            key: vector
            data_type: list
            list:
              method: lpop
            redis_key: redis_key
            "#,
        })
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
        let redis_channel = self.redis_channel.clone().and_then(|k| k.path);

        let client = redis::Client::open(self.url.as_str()).context(ClientSnafu {})?;
        let connection_info = ConnectionInfo::from(client.get_connection_info());
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let bytes_received = register(BytesReceived::from(Protocol::from(
            connection_info.protocol,
        )));
        let events_received = register(EventsReceived);
        let handler = InputHandler {
            client,
            bytes_received: bytes_received.clone(),
            events_received: events_received.clone(),
            key: self.key.clone(),
            redis_key,
            redis_channel,
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
            DataTypeConfig::Pchannel => handler.psubscribe(connection_info).await,
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let redis_key_path = self
            .redis_key
            .clone()
            .and_then(|k| k.path)
            .map(LegacyKey::InsertIfEmpty);

        let mut schema_definition = self
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

        // Add the matched-channel metadata after the standard metadata so the schema is built
        // in the same order `handle_line` inserts fields at runtime. This matters for legacy
        // namespacing: `handle_line` inserts the standard fields first and then the channel
        // with `InsertIfEmpty`, so if `redis_channel` is pointed at an existing field (for
        // example the timestamp), the standard field wins at runtime — and the schema must
        // reflect that same precedence.
        if matches!(self.data_type, DataTypeConfig::Pchannel) {
            let redis_channel_path = self
                .redis_channel
                .clone()
                .and_then(|k| k.path)
                .map(LegacyKey::InsertIfEmpty);

            schema_definition = schema_definition.with_source_metadata(
                Self::NAME,
                redis_channel_path,
                &owned_value_path!("channel"),
                Kind::bytes(),
                None,
            );
        }

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
    pub redis_channel: Option<OwnedValuePath>,
    pub decoder: Decoder,
    pub log_namespace: LogNamespace,
    pub cx: SourceContext,
}

impl InputHandler {
    async fn handle_line(&mut self, line: String, channel: Option<&[u8]>) -> Result<(), ()> {
        let now = Utc::now();

        self.bytes_received.emit(ByteSize(line.len()));

        let mut stream = DecoderFramedRead::new(line.as_ref(), self.decoder.clone());
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
                                Value::from_static_str(RedisSourceConfig::NAME),
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

                            if let Some(channel) = channel {
                                self.log_namespace.insert_source_metadata(
                                    RedisSourceConfig::NAME,
                                    log,
                                    self.redis_channel.as_ref().map(LegacyKey::InsertIfEmpty),
                                    path!("channel"),
                                    channel,
                                );
                            }
                        }

                        event
                    });

                    if (self.cx.out.send_batch(events).await).is_err() {
                        emit(StreamClosedError { count });
                        return Err(());
                    }
                }
                Err(error) => {
                    // Error is logged by `vector_lib::codecs::Decoder`, no further
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

    #[test]
    fn pchannel_config_parses() {
        let config: RedisSourceConfig = serde_yaml::from_str(indoc::indoc! {
            r#"
            url: "redis://127.0.0.1:6379/0"
            key: "news.*"
            data_type: pchannel
            redis_channel: channel
            "#,
        })
        .unwrap();

        assert!(matches!(config.data_type, DataTypeConfig::Pchannel));
        assert!(config.redis_channel.is_some());
    }
}

#[cfg(all(test, feature = "redis-integration-tests"))]
mod integration_test {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };

    use futures::StreamExt;
    use redis::AsyncCommands;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::Notify,
        time::Instant,
    };
    use vrl::value;

    use super::*;
    use crate::{
        SourceSender,
        config::{ComponentKey, log_schema},
        test_util::{
            collect_n, collect_n_stream,
            components::{SOURCE_TAGS, run_and_assert_source_compliance_n},
            random_string,
        },
    };

    fn redis_url() -> String {
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/0".to_owned())
    }

    /// Returns the ids of pub/sub connections currently selected onto database `db`.
    ///
    /// The redis integration tests run concurrently against a shared server, so a reconnect
    /// test can't safely issue a server-wide `CLIENT KILL`. Instead it points its source at a
    /// dedicated database; because no other test uses that database, any pub/sub connection on
    /// it is unambiguously this source's connection, regardless of test timing. (Redis Pub/Sub
    /// is global across databases, so publishing on the default database still reaches it.)
    async fn pubsub_client_ids_on_db(
        conn: &mut redis::aio::MultiplexedConnection,
        db: u32,
    ) -> Vec<u64> {
        let list: String = redis::cmd("CLIENT")
            .arg("LIST")
            .arg("TYPE")
            .arg("pubsub")
            .query_async(conn)
            .await
            .expect("CLIENT LIST should succeed");
        let db_field = format!("db={db}");
        list.lines()
            .filter(|line| line.split(' ').any(|field| field == db_field))
            .filter_map(|line| line.split(' ').next())
            .filter_map(|field| field.strip_prefix("id="))
            .filter_map(|id| id.parse::<u64>().ok())
            .collect()
    }

    #[tokio::test]
    async fn redis_source_list_rpop() {
        // Push some test data into a list object which we'll read from.
        let client = redis::Client::open(redis_url()).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();

        let key = format!("test-key-{}", random_string(10));
        debug!("Test key name: {key}.");

        let _: i32 = conn.rpush(&key, "1").await.unwrap();
        let _: i32 = conn.rpush(&key, "2").await.unwrap();
        let _: i32 = conn.rpush(&key, "3").await.unwrap();

        // Now run the source and make sure we get all three events.
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::List,
            list: Some(ListOption {
                method: Method::Rpop,
            }),
            url: redis_url(),
            key: key.clone(),
            redis_key: None,
            redis_channel: None,
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
        let client = redis::Client::open(redis_url()).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();

        let key = format!("test-key-{}", random_string(10));
        debug!("Test key name: {key}.");

        let _: i32 = conn.rpush(&key, "1").await.unwrap();

        // Now run the source and make sure we get all three events.
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::List,
            list: Some(ListOption {
                method: Method::Rpop,
            }),
            url: redis_url(),
            key: key.clone(),
            redis_key: Some(OptionalValuePath::from(owned_value_path!("remapped_key"))),
            redis_channel: None,
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
        let client = redis::Client::open(redis_url()).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();

        let key = format!("test-key-{}", random_string(10));
        debug!("Test key name: {key}.");

        let _: i32 = conn.rpush(&key, "1").await.unwrap();
        let _: i32 = conn.rpush(&key, "2").await.unwrap();
        let _: i32 = conn.rpush(&key, "3").await.unwrap();

        // Now run the source and make sure we get all three events.
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::List,
            list: Some(ListOption {
                method: Method::Lpop,
            }),
            url: redis_url(),
            key: key.clone(),
            redis_key: None,
            redis_channel: None,
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

    /// Holds a pop reply after Redis removes the entry, then shuts down before delivering it.
    async fn assert_no_entries_lost_on_shutdown(method: Method) {
        let client = redis::Client::open(redis_url()).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();
        let key = format!("test-key-{}", random_string(10));
        let _: i64 = conn.rpush(&key, "held entry").await.unwrap();

        let proxy = freezable_redis_proxy().await;
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::List,
            list: Some(ListOption { method }),
            url: proxy.url.clone(),
            key: key.clone(),
            redis_key: None,
            redis_channel: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            log_namespace: Some(false),
        };
        let (tx, rx) = SourceSender::new_test();
        let (cx, shutdown) = SourceContext::new_shutdown(&ComponentKey::from("redis"), tx);
        let mut source = config.build(cx).await.unwrap();

        // Connection setup has completed, so the next reply belongs to the blocking pop.
        proxy.frozen.store(true, Ordering::SeqCst);
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::select! {
                () = proxy.reply_held.notified() => {}
                result = &mut source => panic!("source stopped before its reply was held: {result:?}"),
            }
        })
        .await
        .expect("proxy did not capture the pop reply");
        let remaining: usize = conn.llen(&key).await.unwrap();
        assert_eq!(remaining, 0, "Redis must have removed the entry");

        // shutdown_all signals synchronously. Poll the source before releasing the reply so it
        // must observe shutdown with a pop in flight, regardless of executor scheduling.
        let shutdown = shutdown.shutdown_all(Some(Instant::now() + Duration::from_secs(10)));
        assert!(
            futures::poll!(&mut source).is_pending(),
            "source abandoned the in-flight pop during shutdown"
        );
        proxy.frozen.store(false, Ordering::SeqCst);
        proxy.release_reply.notify_one();

        tokio::time::timeout(Duration::from_secs(10), async {
            let (result, ()) = tokio::join!(source, shutdown);
            result.unwrap();
        })
        .await
        .expect("source did not finish after its reply was released");
        let events = rx.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].as_log()[log_schema().message_key().unwrap().to_string()],
            "held entry".into()
        );
    }

    #[tokio::test]
    async fn redis_source_list_lpop_loses_no_entries_on_shutdown() {
        assert_no_entries_lost_on_shutdown(Method::Lpop).await;
    }

    #[tokio::test]
    async fn redis_source_list_rpop_loses_no_entries_on_shutdown() {
        assert_no_entries_lost_on_shutdown(Method::Rpop).await;
    }

    struct RedisProxy {
        url: String,
        frozen: Arc<AtomicBool>,
        reply_held: Arc<Notify>,
        release_reply: Arc<Notify>,
    }

    /// Starts a TCP proxy whose replies can be held until explicitly released.
    async fn freezable_redis_proxy() -> RedisProxy {
        let client = redis::Client::open(redis_url()).unwrap();
        let redis::ConnectionAddr::Tcp(host, port) = client.get_connection_info().addr.clone()
        else {
            panic!("test Redis must be reachable over plain TCP");
        };
        let db = client.get_connection_info().redis.db;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_url = format!("redis://{}/{db}", listener.local_addr().unwrap());
        let frozen = Arc::new(AtomicBool::new(false));
        let reply_held = Arc::new(Notify::new());
        let release_reply = Arc::new(Notify::new());

        let frozen_replies = Arc::clone(&frozen);
        let held = Arc::clone(&reply_held);
        let release = Arc::clone(&release_reply);
        tokio::spawn(async move {
            loop {
                let (inbound, _) = listener.accept().await.unwrap();
                let outbound = TcpStream::connect((host.as_str(), port)).await.unwrap();
                let (mut client_read, mut client_write) = inbound.into_split();
                let (mut server_read, mut server_write) = outbound.into_split();
                tokio::spawn(async move {
                    _ = tokio::io::copy(&mut client_read, &mut server_write).await;
                });
                let frozen = Arc::clone(&frozen_replies);
                let held = Arc::clone(&held);
                let release = Arc::clone(&release);
                tokio::spawn(async move {
                    let mut buf = vec![0; 16 * 1024];
                    loop {
                        let Ok(n @ 1..) = server_read.read(&mut buf).await else {
                            return;
                        };
                        if frozen.load(Ordering::SeqCst) {
                            held.notify_one();
                            release.notified().await;
                        }
                        if client_write.write_all(&buf[..n]).await.is_err() {
                            return;
                        }
                    }
                });
            }
        });

        RedisProxy {
            url: proxy_url,
            frozen,
            reply_held,
            release_reply,
        }
    }

    #[tokio::test]
    async fn redis_source_list_shutdown_is_bounded_when_replies_stop() {
        let proxy = freezable_redis_proxy().await;
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::List,
            list: Some(ListOption {
                method: Method::Lpop,
            }),
            url: proxy.url,
            key: format!("test-key-{}", random_string(10)),
            redis_key: None,
            redis_channel: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            log_namespace: Some(false),
        };

        let (tx, rx) = SourceSender::new_test();
        let (cx, shutdown) = SourceContext::new_shutdown(&ComponentKey::from("redis"), tx);
        let source = tokio::spawn(config.build(cx).await.unwrap());
        let _received = tokio::spawn(rx.count());

        // Let the source connect and block on the empty list, then stop delivering replies so
        // its in-flight pop never completes.
        tokio::time::sleep(Duration::from_millis(1500)).await;
        proxy.frozen.store(true, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let stopped = tokio::time::timeout(Duration::from_secs(10), async {
            shutdown
                .shutdown_all(Some(Instant::now() + Duration::from_secs(30)))
                .await;
            source.await.unwrap().unwrap();
        })
        .await;
        assert!(
            stopped.is_ok(),
            "source did not stop while its Redis connection was unresponsive"
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
            url: redis_url(),
            key: key.clone(),
            redis_key: None,
            redis_channel: None,
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
        let client = redis::Client::open(redis_url()).unwrap();

        let mut async_conn = client
            .get_multiplexed_async_connection()
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

    #[tokio::test]
    async fn redis_source_pchannel_consume_event() {
        let prefix = format!("test-pchannel-{}", random_string(10));
        let channel_a = format!("{prefix}-a");
        let channel_b = format!("{prefix}-b");
        // A channel name that is not valid UTF-8. It still matches the pattern (glob matching
        // is byte-wise) and must be preserved verbatim in the `channel` metadata rather than
        // being lossily replaced with `?`.
        let channel_bin = {
            let mut name = format!("{prefix}-").into_bytes();
            name.extend_from_slice(&[0xff, 0xfe]);
            name
        };
        let pattern = format!("{prefix}-*");
        let text = "test message for pchannel";

        // Build and spawn the source before publishing, so the pattern subscription is active.
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::Pchannel,
            list: None,
            url: redis_url(),
            key: pattern.clone(),
            redis_key: None,
            redis_channel: Some(OptionalValuePath::from(owned_value_path!("channel"))),
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
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let client = redis::Client::open(redis_url()).unwrap();
        let mut async_conn = client
            .get_multiplexed_async_connection()
            .await
            .expect("Failed to get redis async connection.");

        let _: i32 = async_conn.publish(channel_a.clone(), text).await.unwrap();
        let _: i32 = async_conn.publish(channel_b.clone(), text).await.unwrap();
        let _: i32 = async_conn
            .publish(channel_bin.clone(), text)
            .await
            .unwrap();

        let events = collect_n(rx, 3).await;
        assert_eq!(events.len(), 3);

        // Collect the matched channel from each event as raw bytes so that the non-UTF-8
        // channel can be compared exactly.
        let mut seen_channels: Vec<Vec<u8>> = Vec::new();
        for event in events {
            let log = event.as_log();
            assert_eq!(
                log[log_schema().message_key().unwrap().to_string()],
                text.into()
            );
            assert_eq!(
                log[log_schema().source_type_key().unwrap().to_string()],
                RedisSourceConfig::NAME.into()
            );
            let channel = log["channel"]
                .as_bytes()
                .expect("channel metadata should be bytes")
                .to_vec();
            seen_channels.push(channel);
        }

        assert!(seen_channels.contains(&channel_a.into_bytes()));
        assert!(seen_channels.contains(&channel_b.into_bytes()));
        assert!(seen_channels.contains(&channel_bin));
    }

    #[tokio::test]
    async fn redis_source_pchannel_reconnect() {
        let prefix = format!("test-pchannel-reconnect-{}", random_string(10));
        let channel = format!("{prefix}-a");
        let pattern = format!("{prefix}-*");
        let text_before = "before reconnect";
        let text_after = "after reconnect";

        // Point the source at a dedicated database (db 1) so its pub/sub connection can be
        // identified unambiguously below; no other integration test uses this database.
        // Derive it from redis_url() so it works whatever REDIS_URL points at.
        const RECONNECT_DB: u32 = 1;
        let source_url = match redis_url().rsplit_once('/') {
            Some((host, _db)) => format!("{host}/{RECONNECT_DB}"),
            None => format!("{}/{RECONNECT_DB}", redis_url()),
        };

        let config = RedisSourceConfig {
            data_type: DataTypeConfig::Pchannel,
            list: None,
            url: source_url,
            key: pattern.clone(),
            redis_key: None,
            redis_channel: Some(OptionalValuePath::from(owned_value_path!("channel"))),
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            log_namespace: Some(false),
        };

        let client = redis::Client::open(redis_url()).unwrap();
        let mut admin = client
            .get_multiplexed_async_connection()
            .await
            .expect("Failed to get redis async connection.");

        let (tx, mut rx) = SourceSender::new_test();
        let context = SourceContext::new_test(tx, None);
        let source = config
            .build(context)
            .await
            .expect("source should not fail to build");

        // The initial connect + PSUBSCRIBE happens during `build()`, so this source's pub/sub
        // connection now exists on the server, selected onto the dedicated database. Since no
        // other test uses that database, this identifies exactly this source's connection.
        let our_ids = pubsub_client_ids_on_db(&mut admin, RECONNECT_DB).await;
        assert!(
            !our_ids.is_empty(),
            "expected the source's pub/sub connection on db {RECONNECT_DB}"
        );

        tokio::spawn(source);

        // Wait for the source to be ready to receive.
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Deliver a message over the initial connection to confirm the source is live.
        let _: i32 = admin.publish(channel.clone(), text_before).await.unwrap();
        let events = collect_n_stream(&mut rx, 1).await;
        assert_eq!(
            events[0].as_log()[log_schema().message_key().unwrap().to_string()],
            text_before.into()
        );

        // Forcibly close ONLY this source's pub/sub connection, simulating a Redis restart or
        // a network blip. The reconnect loop should re-establish the pattern subscription
        // automatically.
        for id in &our_ids {
            let _: redis::Value = redis::cmd("CLIENT")
                .arg("KILL")
                .arg("ID")
                .arg(*id)
                .query_async(&mut admin)
                .await
                .expect("CLIENT KILL ID should succeed");
        }

        // Republish continuously while the source reconnects and re-subscribes. Pub/Sub drops
        // messages published while there is no subscriber, so keep publishing until one is
        // delivered rather than racing a single publish against the reconnect.
        let publisher = {
            let channel = channel.clone();
            let mut conn = client
                .get_multiplexed_async_connection()
                .await
                .expect("Failed to get redis async connection.");
            tokio::spawn(async move {
                loop {
                    let _n: i32 = conn.publish(channel.clone(), text_after).await.unwrap_or(0);
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                }
            })
        };

        // The source delivers this only once it has reconnected and re-subscribed.
        let events = collect_n_stream(&mut rx, 1).await;
        publisher.abort();

        let log = events[0].as_log();
        assert_eq!(
            log[log_schema().message_key().unwrap().to_string()],
            text_after.into()
        );
        // The matched channel metadata is still populated after reconnecting.
        let channel_meta = log["channel"]
            .as_bytes()
            .expect("channel metadata should be bytes")
            .to_vec();
        assert_eq!(channel_meta, channel.into_bytes());
    }
}
