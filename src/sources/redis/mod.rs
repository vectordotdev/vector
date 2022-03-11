use crate::{
    codecs::{
        self,
        decoding::{DecodingConfig, DeserializerConfig, FramingConfig},
    },
    config::{
        log_schema, DataType, GenerateConfig, Output, SourceConfig, SourceContext,
        SourceDescription,
    },
    event::Event,
    internal_events::{BytesReceived, EventsReceived, StreamClosedError},
    serde::{default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    sources::util::StreamDecodingError,
    SourceSender,
};
use bytes::Bytes;
use chrono::Utc;
use futures::StreamExt;
use redis::{Client, RedisResult};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::FramedRead;
use vector_core::ByteSizeOf;

mod channel;
mod list;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to build redis client: {}", source))]
    Client { source: redis::RedisError },
}

#[derive(Copy, Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum DataTypeConfig {
    #[derivative(Default)]
    List,
    Channel,
}

#[derive(Copy, Clone, Debug, Default, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub struct ListOption {
    method: Method,
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    #[derivative(Default)]
    Lpop,
    Rpop,
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RedisSourceConfig {
    #[serde(default)]
    data_type: DataTypeConfig,
    list: Option<ListOption>,
    url: String,
    key: String,
    redis_key: Option<String>,
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    framing: FramingConfig,
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    decoding: DeserializerConfig,
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

inventory::submit! {
    SourceDescription::new::<RedisSourceConfig>("redis")
}

#[async_trait::async_trait]
#[typetag::serde(name = "redis")]
impl SourceConfig for RedisSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build();
        redis_source(self, decoder, cx.shutdown, cx.out).await
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "redis"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

async fn redis_source(
    config: &RedisSourceConfig,
    decoder: codecs::Decoder,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<super::Source> {
    if config.key.is_empty() {
        return Err("`key` cannot be empty.".into());
    }

    let client = build_client(config).context(ClientSnafu {})?;

    match config.data_type {
        DataTypeConfig::List => {
            let list = config.list.unwrap_or_default();
            list::watch(
                client,
                config.key.clone(),
                config.redis_key.clone(),
                list.method,
                decoder,
                shutdown,
                out,
            )
            .await
        }
        DataTypeConfig::Channel => {
            channel::subscribe(
                client,
                config.key.clone(),
                config.redis_key.clone(),
                decoder,
                shutdown,
                out,
            )
            .await
        }
    }
}

fn build_client(config: &RedisSourceConfig) -> RedisResult<Client> {
    trace!("Opening redis client.");
    let client = redis::Client::open(config.url.as_str());
    trace!("Opened redis client.");
    client
}

async fn handle_line(
    line: String,
    key: &str,
    redis_key: Option<&str>,
    decoder: codecs::Decoder,
    out: &mut SourceSender,
) -> Result<(), ()> {
    let now = Utc::now();

    emit!(&BytesReceived {
        byte_size: line.len(),
        protocol: "tcp",
    });
    let mut stream = FramedRead::new(line.as_ref(), decoder.clone());
    while let Some(next) = stream.next().await {
        match next {
            Ok((events, _byte_size)) => {
                let count = events.len();
                emit!(&EventsReceived {
                    byte_size: events.size_of(),
                    count,
                });

                let events = events.into_iter().map(|mut event| {
                    if let Event::Log(ref mut log) = event {
                        log.try_insert(log_schema().source_type_key(), Bytes::from("redis"));
                        log.try_insert(log_schema().timestamp_key(), now);
                        if let Some(redis_key) = redis_key {
                            event.as_mut_log().insert(redis_key, key);
                        }
                    }
                    event
                });

                if let Err(error) = out.send_batch(events).await {
                    emit!(&StreamClosedError { error, count });
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RedisSourceConfig>();
    }
}

#[cfg(feature = "redis-integration-tests")]
#[cfg(test)]
mod integration_test {
    use super::*;
    use crate::config::log_schema;
    use crate::{
        shutdown::ShutdownSignal,
        test_util::{collect_n, random_string},
        SourceSender,
    };
    use redis::AsyncCommands;

    const REDIS_SERVER: &str = "redis://redis:6379/0";

    #[tokio::test]
    async fn redis_source_list_rpop() {
        let client = redis::Client::open(REDIS_SERVER).unwrap();
        let mut conn = client.get_tokio_connection_manager().await.unwrap();

        let key = format!("test-key-{}", random_string(10));
        debug!("Test key name: {}.", key);

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
        };

        let _: i32 = conn.rpush(&key, "1").await.unwrap();
        let _: i32 = conn.rpush(&key, "2").await.unwrap();
        let _: i32 = conn.rpush(&key, "3").await.unwrap();

        let (tx, rx) = SourceSender::new_test();
        tokio::spawn(
            redis_source(
                &config,
                codecs::Decoder::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .await
            .unwrap(),
        );
        let events = collect_n(rx, 3).await;

        assert_eq!(events[0].as_log()[log_schema().message_key()], "3".into());
        assert_eq!(events[1].as_log()[log_schema().message_key()], "2".into());
        assert_eq!(events[2].as_log()[log_schema().message_key()], "1".into());
    }

    #[tokio::test]
    async fn redis_source_list_lpop() {
        let client = redis::Client::open(REDIS_SERVER).unwrap();
        let mut conn = client.get_tokio_connection_manager().await.unwrap();

        let key = format!("test-key-{}", random_string(10));
        debug!("Test key name: {}.", key);

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
        };

        let _: i32 = conn.rpush(&key, "1").await.unwrap();
        let _: i32 = conn.rpush(&key, "2").await.unwrap();
        let _: i32 = conn.rpush(&key, "3").await.unwrap();

        let (tx, rx) = SourceSender::new_test();
        tokio::spawn(
            redis_source(
                &config,
                codecs::Decoder::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .await
            .unwrap(),
        );
        let events = collect_n(rx, 3).await;

        assert_eq!(events[0].as_log()[log_schema().message_key()], "1".into());
        assert_eq!(events[1].as_log()[log_schema().message_key()], "2".into());
        assert_eq!(events[2].as_log()[log_schema().message_key()], "3".into());
    }

    #[tokio::test]
    async fn redis_source_channel_consume_event() {
        let key = "test-channel".to_owned();
        debug!("Test key name: {}.", key);

        let text = "test message for channel";

        let config = RedisSourceConfig {
            data_type: DataTypeConfig::Channel,
            list: None,
            url: REDIS_SERVER.to_owned(),
            key: key.clone(),
            redis_key: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
        };

        debug!("Receiving event.");
        let (tx, rx) = SourceSender::new_test();
        tokio::spawn(
            redis_source(
                &config,
                codecs::Decoder::default(),
                ShutdownSignal::noop(),
                tx,
            )
            .await
            .unwrap(),
        );

        let client = redis::Client::open(REDIS_SERVER).unwrap();

        let mut async_conn = client
            .get_async_connection()
            .await
            .expect("Failed to get redis async connection.");

        // wait for redis_source subscribe the channel.
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        for _i in 0..10000 {
            let _: i32 = async_conn.publish(key.clone(), text).await.unwrap();
        }

        let events = collect_n(rx, 10000).await;

        assert_eq!(events.len(), 10000);

        for event in events.iter().take(10000) {
            assert_eq!(event.as_log()[log_schema().message_key()], text.into());
            assert_eq!(
                event.as_log()[log_schema().source_type_key()],
                "redis".into()
            );
        }
    }
}
