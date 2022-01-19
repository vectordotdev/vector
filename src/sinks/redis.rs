use std::{
    convert::TryFrom,
    num::NonZeroU64,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, stream, FutureExt, SinkExt, StreamExt};
use redis::{aio::ConnectionManager, RedisError, RedisResult};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tower::{Service, ServiceBuilder};
use vector_core::ByteSizeOf;

use super::util::SinkBatchSettings;
use crate::{
    config::{self, log_schema, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    internal_events::{RedisEventSent, RedisSendEventFailed, TemplateRenderingFailed},
    sinks::util::{
        batch::BatchConfig,
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::{RetryAction, RetryLogic},
        sink::Response,
        BatchSink, Concurrency, EncodedEvent, EncodedLength, ServiceBuilderExt, TowerRequestConfig,
        VecBuffer,
    },
    template::{Template, TemplateParseError},
};

inventory::submit! {
    SinkDescription::new::<RedisSinkConfig>("redis")
}

#[derive(Debug, Snafu)]
enum RedisSinkError {
    #[snafu(display("Creating Redis producer failed: {}", source))]
    RedisCreateFailed { source: RedisError },
    #[snafu(display("Invalid key template: {}", source))]
    KeyTemplate { source: TemplateParseError },
}

#[derive(Copy, Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum DataTypeConfig {
    #[derivative(Default)]
    List,
    Channel,
}

#[derive(Copy, Clone, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub struct ListOption {
    method: Method,
}

#[derive(Copy, Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    #[derivative(Default)]
    List(Method),
    Channel,
}

#[derive(Copy, Clone, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    #[derivative(Default)]
    RPush,
    LPush,
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RedisDefaultBatchSettings;

impl SinkBatchSettings for RedisDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(1) };
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RedisSinkConfig {
    encoding: EncodingConfig<Encoding>,
    #[serde(default)]
    data_type: DataTypeConfig,
    #[serde(alias = "list")]
    list_option: Option<ListOption>,
    url: String,
    key: String,
    #[serde(default)]
    batch: BatchConfig<RedisDefaultBatchSettings>,
    #[serde(default)]
    request: TowerRequestConfig,
}

impl GenerateConfig for RedisSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            url = "redis://127.0.0.1:6379/0"
            key = "vector"
            data_type = "list"
            list.method = "lpush"
            encoding.codec = "json"
            batch.max_events = 1
            "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "redis")]
impl SinkConfig for RedisSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        if self.key.is_empty() {
            return Err("`key` cannot be empty.".into());
        }
        let conn = self.build_client().await.context(RedisCreateFailedSnafu)?;
        let healthcheck = RedisSinkConfig::healthcheck(conn.clone()).boxed();
        let sink = self.new(conn, cx)?;
        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> config::DataType {
        config::DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "redis"
    }
}

impl RedisSinkConfig {
    pub fn new(
        &self,
        conn: ConnectionManager,
        cx: SinkContext,
    ) -> crate::Result<super::VectorSink> {
        let request = self.request.unwrap_with(&TowerRequestConfig {
            concurrency: Concurrency::Fixed(1),
            ..Default::default()
        });

        let key = Template::try_from(self.key.clone()).context(KeyTemplateSnafu)?;
        let encoding = self.encoding.clone();

        let method = self.list_option.map(|option| option.method);

        let data_type = match self.data_type {
            DataTypeConfig::Channel => DataType::Channel,
            DataTypeConfig::List => DataType::List(method.unwrap_or_default()),
        };

        let batch = self.batch.into_batch_settings()?;

        let buffer = VecBuffer::new(batch.size);

        let redis = RedisSink { conn, data_type };

        let svc = ServiceBuilder::new()
            .settings(request, RedisRetryLogic)
            .service(redis);

        let sink = BatchSink::new(svc, buffer, batch.timeout, cx.acker())
            .with_flat_map(move |e| stream::iter(encode_event(e, &key, &encoding)).map(Ok))
            .sink_map_err(|error| error!(message = "Sink failed to flush.", %error));

        Ok(super::VectorSink::from_event_sink(sink))
    }

    async fn build_client(&self) -> RedisResult<ConnectionManager> {
        trace!("Open Redis client.");
        let client = redis::Client::open(self.url.as_str())?;
        trace!("Open Redis client success.");
        trace!("Get Redis connection.");
        let conn = client.get_tokio_connection_manager().await;
        trace!("Get Redis connection success.");
        conn
    }
    async fn healthcheck(mut conn: ConnectionManager) -> crate::Result<()> {
        redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(Into::into)
    }
}

#[derive(Debug, Clone)]
struct RedisKvEntry {
    key: String,
    value: Vec<u8>,
}

impl EncodedLength for RedisKvEntry {
    fn encoded_length(&self) -> usize {
        self.value.len()
    }
}

impl ByteSizeOf for RedisKvEntry {
    fn allocated_bytes(&self) -> usize {
        self.key.len() + self.value.len()
    }
}

fn encode_event(
    mut event: Event,
    key: &Template,
    encoding: &EncodingConfig<Encoding>,
) -> Option<EncodedEvent<RedisKvEntry>> {
    let key = key
        .render_string(&event)
        .map_err(|error| {
            emit!(&TemplateRenderingFailed {
                error,
                field: Some("key"),
                drop_event: true,
            });
        })
        .ok()?;

    let byte_size = event.size_of();
    encoding.apply_rules(&mut event);

    let value = match encoding.codec() {
        Encoding::Json => serde_json::to_vec(event.as_log())
            .map_err(|error| panic!("Unable to encode into JSON: {}", error))
            .unwrap_or_default(),
        Encoding::Text => event
            .as_log()
            .get(log_schema().message_key())
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or_default(),
    };

    let event = EncodedEvent::new(RedisKvEntry { key, value }, byte_size);
    Some(event)
}

type RedisPipeResult = RedisResult<Vec<bool>>;

impl Response for Vec<bool> {
    fn is_successful(&self) -> bool {
        self.iter().all(|x| *x)
    }
}

#[derive(Debug, Clone)]
struct RedisRetryLogic;

impl RetryLogic for RedisRetryLogic {
    type Error = RedisError;
    type Response = Vec<bool>;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        if response.is_successful() {
            return RetryAction::Successful;
        }
        RetryAction::Retry("Sending data to redis failed.".into())
    }
}

#[derive(Clone)]
pub struct RedisSink {
    conn: ConnectionManager,
    data_type: DataType,
}

impl Service<Vec<RedisKvEntry>> for RedisSink {
    type Response = Vec<bool>;
    type Error = RedisError;
    type Future = BoxFuture<'static, RedisPipeResult>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, kvs: Vec<RedisKvEntry>) -> Self::Future {
        let count = kvs.len();
        let mut byte_size = 0;

        let mut conn = self.conn.clone();
        let mut pipe = redis::pipe();

        for kv in kvs {
            byte_size += kv.encoded_length();
            match self.data_type {
                DataType::List(method) => match method {
                    Method::LPush => {
                        if count > 1 {
                            pipe.atomic().lpush(kv.key, kv.value);
                        } else {
                            pipe.lpush(kv.key, kv.value);
                        }
                    }
                    Method::RPush => {
                        if count > 1 {
                            pipe.atomic().rpush(kv.key, kv.value);
                        } else {
                            pipe.rpush(kv.key, kv.value);
                        }
                    }
                },
                DataType::Channel => {
                    if count > 1 {
                        pipe.atomic().publish(kv.key, kv.value);
                    } else {
                        pipe.publish(kv.key, kv.value);
                    }
                }
            }
        }

        Box::pin(async move {
            let result: RedisPipeResult = pipe.query_async(&mut conn).await;
            match &result {
                Ok(res) => {
                    if res.is_successful() {
                        emit!(&RedisEventSent { count, byte_size });
                    } else {
                        warn!("Batch sending was not all successful and will be retried.")
                    }
                }
                Err(error) => emit!(&RedisSendEventFailed {
                    error: error.to_string()
                }),
            };
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, convert::TryFrom};

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RedisSinkConfig>();
    }

    #[test]
    fn redis_event_json() {
        let msg = "hello_world".to_owned();
        let mut evt = Event::from(msg.clone());
        evt.as_mut_log().insert("key", "value");
        let result = encode_event(
            evt,
            &Template::try_from("key").unwrap(),
            &EncodingConfig::from(Encoding::Json),
        )
        .unwrap()
        .item
        .value;
        let map: HashMap<String, String> = serde_json::from_slice(&result[..]).unwrap();
        assert_eq!(msg, map[&log_schema().message_key().to_string()]);
    }

    #[test]
    fn redis_event_text() {
        let msg = "hello_world".to_owned();
        let evt = Event::from(msg.clone());
        let event = encode_event(
            evt,
            &Template::try_from("key").unwrap(),
            &EncodingConfig::from(Encoding::Text),
        )
        .unwrap()
        .item
        .value;
        assert_eq!(event, Vec::from(msg));
    }

    #[test]
    fn redis_encode_event() {
        let msg = "hello_world";
        let mut evt = Event::from(msg);
        evt.as_mut_log().insert("key", "value");

        let result = encode_event(
            evt,
            &Template::try_from("key").unwrap(),
            &EncodingConfig {
                codec: Encoding::Json,
                schema: None,
                only_fields: None,
                except_fields: Some(vec!["key".into()]),
                timestamp_format: None,
            },
        )
        .unwrap()
        .item
        .value;

        let map: HashMap<String, String> = serde_json::from_slice(&result[..]).unwrap();
        assert!(!map.contains_key("key"));
    }
}

#[cfg(feature = "redis-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use rand::Rng;
    use redis::AsyncCommands;

    use super::*;
    use crate::test_util::{random_lines_with_stream, random_string, trace_init};

    fn redis_server() -> String {
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/0".to_owned())
    }

    #[tokio::test]
    async fn redis_sink_list_lpush() {
        trace_init();

        let key = format!("test-{}", random_string(10));
        debug!("Test key name: {}.", key);
        let mut rng = rand::thread_rng();
        let num_events = rng.gen_range(10000..20000);
        debug!("Test events num: {}.", num_events);

        let cnf = RedisSinkConfig {
            url: redis_server(),
            key: key.clone(),
            encoding: Encoding::Json.into(),
            data_type: DataTypeConfig::List,
            list_option: Some(ListOption {
                method: Method::LPush,
            }),
            batch: BatchConfig::default(),
            request: TowerRequestConfig {
                rate_limit_num: Option::from(u64::MAX),
                ..Default::default()
            },
        };

        // Publish events.
        let conn = cnf.build_client().await.unwrap();
        let cx = SinkContext::new_test();

        let sink = cnf.new(conn, cx).unwrap();

        let mut events: Vec<Event> = Vec::new();
        for i in 0..num_events {
            let s: String = i.to_string();
            let e = Event::from(s);
            events.push(e);
        }

        sink.run_events(events.clone()).await.unwrap();

        let mut conn = cnf.build_client().await.unwrap();

        let key_exists: bool = conn.exists(key.clone()).await.unwrap();
        debug!("Test key: {} exists: {}.", key, key_exists);
        assert!(key_exists);
        let llen: usize = conn.llen(key.clone()).await.unwrap();
        debug!("Test key: {} len: {}.", key, llen);
        assert_eq!(llen, num_events);

        for i in 0..num_events {
            let e = events.get(i).unwrap().as_log();
            let s = serde_json::to_string(e).unwrap_or_default();
            let payload: (String, String) = conn.brpop(key.clone(), 2000).await.unwrap();
            let val = payload.1;
            assert_eq!(val, s);
        }
    }

    #[tokio::test]
    async fn redis_sink_list_rpush() {
        trace_init();

        let key = format!("test-{}", random_string(10));
        debug!("Test key name: {}.", key);
        let mut rng = rand::thread_rng();
        let num_events = rng.gen_range(10000..20000);
        debug!("Test events num: {}.", num_events);

        let cnf = RedisSinkConfig {
            url: redis_server(),
            key: key.clone(),
            encoding: Encoding::Json.into(),
            data_type: DataTypeConfig::List,
            list_option: Some(ListOption {
                method: Method::RPush,
            }),
            batch: BatchConfig::default(),
            request: TowerRequestConfig {
                rate_limit_num: Option::from(u64::MAX),
                ..Default::default()
            },
        };

        // Publish events.
        let conn = cnf.build_client().await.unwrap();
        let cx = SinkContext::new_test();

        let sink = cnf.new(conn, cx).unwrap();
        let mut events: Vec<Event> = Vec::new();
        for i in 0..num_events {
            let s: String = i.to_string();
            let e = Event::from(s);
            events.push(e);
        }

        sink.run_events(events.clone()).await.unwrap();

        let mut conn = cnf.build_client().await.unwrap();

        let key_exists: bool = conn.exists(key.clone()).await.unwrap();
        debug!("Test key: {} exists: {}.", key, key_exists);
        assert!(key_exists);
        let llen: usize = conn.llen(key.clone()).await.unwrap();
        debug!("Test key: {} len: {}.", key, llen);
        assert_eq!(llen, num_events);

        for i in 0..num_events {
            let e = events.get(i).unwrap().as_log();
            let s = serde_json::to_string(e).unwrap_or_default();
            let payload: (String, String) = conn.blpop(key.clone(), 2000).await.unwrap();
            let val = payload.1;
            assert_eq!(val, s);
        }
    }

    #[tokio::test]
    async fn redis_sink_channel() {
        trace_init();

        let key = format!("test-{}", random_string(10));
        debug!("Test key name: {}.", key);
        let mut rng = rand::thread_rng();
        let num_events = rng.gen_range(10000..20000);
        debug!("Test events num: {}.", num_events);

        let client = redis::Client::open(redis_server()).unwrap();
        debug!("Get Redis async connection.");
        let conn = client
            .get_async_connection()
            .await
            .expect("Failed to get Redis async connection.");
        debug!("Get Redis async connection success.");
        let mut pubsub_conn = conn.into_pubsub();
        debug!("Subscribe channel:{}.", key.as_str());
        pubsub_conn
            .subscribe(key.as_str())
            .await
            .unwrap_or_else(|_| panic!("Failed to subscribe channel:{}.", key.as_str()));
        debug!("Subscribed to channel:{}.", key.as_str());
        let mut pubsub_stream = pubsub_conn.on_message();

        let cnf = RedisSinkConfig {
            url: redis_server(),
            key: key.clone(),
            encoding: Encoding::Json.into(),
            data_type: DataTypeConfig::Channel,
            list_option: None,
            batch: BatchConfig::default(),
            request: TowerRequestConfig {
                rate_limit_num: Option::from(u64::MAX),
                ..Default::default()
            },
        };

        // Publish events.
        let conn = cnf.build_client().await.unwrap();
        let cx = SinkContext::new_test();

        let sink = cnf.new(conn, cx).unwrap();
        let (_input, events) = random_lines_with_stream(100, num_events, None);
        sink.run(events).await.unwrap();

        // Receive events.
        let mut received_msg_num = 0;
        loop {
            let _msg = pubsub_stream.next().await.unwrap();
            received_msg_num += 1;
            debug!("Received msg num:{}.", received_msg_num);
            if received_msg_num == num_events {
                assert_eq!(received_msg_num, num_events);
                break;
            }
        }
    }
}
