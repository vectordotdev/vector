use std::num::NonZeroUsize;

use chrono::Utc;
use futures::StreamExt;
use snafu::{ResultExt, Snafu};
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
}

/// Options for the Redis `list` data type.
#[configurable_component]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub struct ListOption {
    method: Method,

    /// The maximum number of entries to read from the list per round trip.
    ///
    /// The source blocks until at least one entry is available, then reads up to `batch_size - 1`
    /// more without waiting, so batches only fill up when entries are queued. Values greater than
    /// `1` use the `count` argument of `LPOP`/`RPOP` (Redis 6.2 or newer). On older servers the
    /// source falls back to pipelining single pops, which still saves round trips.
    ///
    /// Entries are removed from the list as they are read, so up to `batch_size` entries can be
    /// lost if Vector stops unexpectedly before they are processed.
    #[serde(default = "default_batch_size")]
    #[configurable(metadata(docs::examples = 100))]
    batch_size: NonZeroUsize,
}

const fn default_batch_size() -> NonZeroUsize {
    NonZeroUsize::MIN
}

impl Default for ListOption {
    fn default() -> Self {
        Self {
            method: Method::default(),
            batch_size: default_batch_size(),
        }
    }
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
    /// The Redis data type (`list` or `channel`) to use.
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
            DataTypeConfig::List => handler.watch(self.list.unwrap_or_default()).await,
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
    /// Decodes and emits each entry in order; stops if downstream has closed.
    async fn handle_lines(&mut self, lines: Vec<String>) -> Result<(), ()> {
        for line in lines {
            self.handle_line(line).await?;
        }
        Ok(())
    }

    async fn handle_line(&mut self, line: String) -> Result<(), ()> {
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
                        }

                        event
                    });

                    if (self.cx.out.send_batch(events).await).is_err() {
                        emit!(StreamClosedError { count });
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

    fn parse_list_option(yaml: &str) -> Result<ListOption, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    #[test]
    fn list_batch_size_defaults_to_one() {
        let list = parse_list_option("method: lpop").unwrap();
        assert_eq!(list.batch_size.get(), 1);
    }

    #[test]
    fn list_batch_size_is_configurable() {
        let list = parse_list_option("method: rpop\nbatch_size: 100").unwrap();
        assert_eq!(list.method, Method::Rpop);
        assert_eq!(list.batch_size.get(), 100);
    }

    #[test]
    fn list_batch_size_rejects_zero() {
        assert!(parse_list_option("method: lpop\nbatch_size: 0").is_err());
    }

    #[test]
    fn list_option_default_has_batch_size_one() {
        assert_eq!(ListOption::default().batch_size.get(), 1);
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
            collect_n,
            components::{SOURCE_TAGS, run_and_assert_source_compliance_n},
            random_string,
        },
    };

    fn redis_url() -> String {
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/0".to_owned())
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
                ..Default::default()
            }),
            url: redis_url(),
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
                ..Default::default()
            }),
            url: redis_url(),
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
                ..Default::default()
            }),
            url: redis_url(),
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

    async fn assert_list_batches_in_order(method: Method, expected: Vec<String>) {
        let client = redis::Client::open(redis_url()).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();

        let key = format!("test-key-{}", random_string(10));
        for i in 1..=1000 {
            let _: i64 = conn.rpush(&key, i.to_string()).await.unwrap();
        }

        let config = RedisSourceConfig {
            data_type: DataTypeConfig::List,
            list: Some(ListOption {
                method,
                batch_size: NonZeroUsize::new(100).unwrap(),
            }),
            url: redis_url(),
            key: key.clone(),
            redis_key: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            log_namespace: Some(false),
        };

        let events = run_and_assert_source_compliance_n(config, 1000, &SOURCE_TAGS).await;

        let messages: Vec<String> = events
            .iter()
            .map(|event| {
                event.as_log()[log_schema().message_key().unwrap().to_string()]
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(messages, expected);

        let remaining: usize = conn.llen(&key).await.unwrap();
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn redis_source_list_lpop_in_batches() {
        let expected = (1..=1000).map(|i| i.to_string()).collect();
        assert_list_batches_in_order(Method::Lpop, expected).await;
    }

    #[tokio::test]
    async fn redis_source_list_rpop_in_batches() {
        let expected = (1..=1000).rev().map(|i| i.to_string()).collect();
        assert_list_batches_in_order(Method::Rpop, expected).await;
    }

    /// Holds a pop reply after Redis removes the entry, then shuts down before delivering it.
    async fn assert_no_entries_lost_on_shutdown(method: Method, batch_size: usize) {
        let client = redis::Client::open(redis_url()).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();
        let key = format!("test-key-{}", random_string(10));
        let _: i64 = conn.rpush(&key, "held entry").await.unwrap();

        let proxy = freezable_redis_proxy().await;
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::List,
            list: Some(ListOption {
                method,
                batch_size: NonZeroUsize::new(batch_size).unwrap(),
            }),
            url: proxy.url.clone(),
            key: key.clone(),
            redis_key: None,
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
        assert_no_entries_lost_on_shutdown(Method::Lpop, 1).await;
    }

    #[tokio::test]
    async fn redis_source_list_rpop_loses_no_entries_on_shutdown() {
        assert_no_entries_lost_on_shutdown(Method::Rpop, 1).await;
    }

    #[tokio::test]
    async fn redis_source_list_in_batches_loses_no_entries_on_shutdown() {
        assert_no_entries_lost_on_shutdown(Method::Lpop, 10).await;
    }

    /// On servers without `LPOP key count` (older than Redis 6.2), holds the rejected count
    /// probe's reply, shuts down, then releases it: the source must not start the pipelined
    /// fallback after shutdown, so the entries behind the first one stay in the list.
    #[tokio::test]
    async fn redis_source_list_fallback_does_not_read_after_shutdown() {
        let client = redis::Client::open(redis_url()).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();
        if list::integration_test::server_supports_pop_count(&mut conn).await {
            return;
        }
        let key = format!("test-key-{}", random_string(10));
        for value in ["1", "2", "3"] {
            let _: i64 = conn.rpush(&key, value).await.unwrap();
        }

        let proxy = freezable_redis_proxy().await;
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::List,
            list: Some(ListOption {
                method: Method::Lpop,
                batch_size: NonZeroUsize::new(10).unwrap(),
            }),
            url: proxy.url.clone(),
            key: key.clone(),
            redis_key: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            log_namespace: Some(false),
        };
        let (tx, rx) = SourceSender::new_test();
        let (cx, shutdown) = SourceContext::new_shutdown(&ComponentKey::from("redis"), tx);
        let mut source = config.build(cx).await.unwrap();

        // Hold the blocking pop's reply, let it through, then hold the count probe's reply.
        proxy.frozen.store(true, Ordering::SeqCst);
        for reply in ["blocking pop", "count probe"] {
            tokio::time::timeout(Duration::from_secs(10), async {
                tokio::select! {
                    () = proxy.reply_held.notified() => {}
                    result = &mut source => panic!("source stopped before the {reply} reply: {result:?}"),
                }
            })
            .await
            .unwrap_or_else(|_| panic!("proxy did not capture the {reply} reply"));
            if reply == "blocking pop" {
                proxy.release_reply.notify_one();
            }
        }
        let remaining: usize = conn.llen(&key).await.unwrap();
        assert_eq!(
            remaining, 2,
            "only the blocking pop may have removed an entry"
        );

        let shutdown = shutdown.shutdown_all(Some(Instant::now() + Duration::from_secs(10)));
        assert!(futures::poll!(&mut source).is_pending());
        proxy.frozen.store(false, Ordering::SeqCst);
        proxy.release_reply.notify_one();

        tokio::time::timeout(Duration::from_secs(10), async {
            let (result, ()) = tokio::join!(source, shutdown);
            result.unwrap();
        })
        .await
        .expect("source did not finish after the probe reply was released");
        let events = rx.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 1, "no entries may be read after shutdown");
        let remaining: usize = conn.llen(&key).await.unwrap();
        assert_eq!(remaining, 2, "no pipelined pops may be sent after shutdown");
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

    /// Stops the source after its Redis replies stop arriving, with `entries` queued in the list
    /// (0 leaves the source blocked on an empty list), and checks that shutdown still finishes.
    async fn assert_shutdown_is_bounded_when_replies_stop(batch_size: usize, entries: usize) {
        let client = redis::Client::open(redis_url()).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();
        let key = format!("test-key-{}", random_string(10));
        let values: Vec<String> = (1..=entries).map(|i| i.to_string()).collect();
        for chunk in values.chunks(1000) {
            let _: i64 = conn.rpush(&key, chunk).await.unwrap();
        }

        let proxy = freezable_redis_proxy().await;
        let config = RedisSourceConfig {
            data_type: DataTypeConfig::List,
            list: Some(ListOption {
                method: Method::Lpop,
                batch_size: NonZeroUsize::new(batch_size).unwrap(),
            }),
            url: proxy.url,
            key: key.clone(),
            redis_key: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            log_namespace: Some(false),
        };

        let (tx, rx) = SourceSender::new_test();
        let (cx, shutdown) = SourceContext::new_shutdown(&ComponentKey::from("redis"), tx);
        let source = tokio::spawn(config.build(cx).await.unwrap());
        let _received = tokio::spawn(rx.count());

        // Let the source connect and start reading, then stop delivering replies so its
        // in-flight read never completes.
        if entries == 0 {
            tokio::time::sleep(Duration::from_millis(1500)).await;
        } else {
            loop {
                let remaining: usize = conn.llen(&key).await.unwrap();
                if remaining < entries {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
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
    async fn redis_source_list_shutdown_is_bounded_when_replies_stop() {
        assert_shutdown_is_bounded_when_replies_stop(1, 0).await;
    }

    #[tokio::test]
    async fn redis_source_list_in_batches_shutdown_is_bounded_when_replies_stop() {
        assert_shutdown_is_bounded_when_replies_stop(100, 50_000).await;
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
}
