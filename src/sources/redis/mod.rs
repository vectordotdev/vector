use crate::{
    config::{log_schema, DataType, GlobalOptions, SourceConfig, SourceDescription},
    event::Event,
    shutdown::ShutdownSignal,
    Pipeline,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

mod channel;
mod list;

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    #[derivative(Default)]
    List,
    Channel,
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    #[derivative(Default)]
    BRPOP,
    BLPOP,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RedisSourceConfig {
    #[serde(default)]
    pub data_type: Type,
    pub method: Option<Method>,
    pub url: String,
    pub key: String,
    pub redis_key: Option<String>,
}

impl Default for RedisSourceConfig {
    fn default() -> Self {
        RedisSourceConfig {
            data_type: Type::List,
            url: String::from("redis://127.0.0.1:6379/0"),
            key: String::from("vector"),
            method: Some(Method::BRPOP),
            redis_key: Some("redis_key".to_owned()),
        }
    }
}

inventory::submit! {
    SourceDescription::new::<RedisSourceConfig>("redis")
}

impl_generate_config_from_default!(RedisSourceConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "redis")]
impl SourceConfig for RedisSourceConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        redis_source(self, shutdown, out)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "redis"
    }
}

fn redis_source(
    config: &RedisSourceConfig,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> crate::Result<super::Source> {
    if config.key.is_empty() {
        return Err("`key` cannot be empty.".into());
    } else if let Type::List = config.data_type {
        if config.method.is_none() {
            return Err("When `data_type` is `list`, `method` cannot be empty.".into());
        }
    }

    let client = build_client(config).expect("Failed to open redis client.");
    let key = config.key.clone();
    let redis_key = config.redis_key.clone();

    match config.data_type {
        Type::List => match config.method {
            Some(method) => Ok(list::watch(client, key, redis_key, method, shutdown, out)),
            None => {
                panic!("When `data_type` is `list`, `method` cannot be empty.")
            }
        },
        Type::Channel => Ok(channel::subscribe(client, key, redis_key, shutdown, out)),
    }
}

fn build_client(config: &RedisSourceConfig) -> crate::Result<redis::Client> {
    trace!("Open redis client.");
    let client = redis::Client::open(config.url.as_str())?;
    trace!("Open redis client successed.");
    Ok(client)
}

fn create_event(line: &str, key: String, redis_key: &Option<String>) -> Event {
    let mut event = Event::from(line);
    event
        .as_mut_log()
        .insert(log_schema().source_type_key(), Bytes::from("redis"));
    if let Some(redis_key) = &redis_key {
        event.as_mut_log().insert(redis_key.clone(), key);
    }
    event
}

#[cfg(test)]
mod test {
    use super::{redis_source, RedisSourceConfig};
    use crate::sources::redis::{Method, Type};
    use crate::{shutdown::ShutdownSignal, Pipeline};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RedisSourceConfig>();
    }

    fn make_config(t: Type) -> RedisSourceConfig {
        RedisSourceConfig {
            data_type: t,
            url: String::from("redis://127.0.0.1:6379/0"),
            key: String::from("vector"),
            method: Option::from(Method::BRPOP),
            redis_key: None,
        }
    }

    #[test]
    fn redis_list_source_create_ok() {
        let config = make_config(Type::List);
        assert!(redis_source(&config, ShutdownSignal::noop(), Pipeline::new_test().0).is_ok());
    }

    #[test]
    fn redis_channel_source_create_ok() {
        let config = make_config(Type::Channel);
        assert!(redis_source(&config, ShutdownSignal::noop(), Pipeline::new_test().0).is_ok());
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
        Pipeline,
    };
    use redis::{AsyncCommands, RedisResult};

    const REDIS_SERVER: &str = "redis://127.0.0.1:6379/0";

    async fn send_event_by_list(key: String, text: &str) {
        let client = redis::Client::open(REDIS_SERVER).unwrap();
        trace!("Get redis connection manager.");
        let mut conn = client
            .get_tokio_connection_manager()
            .await
            .expect("Failed to get redis async connection.");
        trace!("Get redis connection manager success.");
        let res: RedisResult<i32> = conn.lpush(key, text).await;
        match res {
            Ok(len) => {
                debug!("Send event for list success, len: {:?}.", len);
            }
            Err(err) => {
                panic!("Send event for list error: {:?}.", err);
            }
        }
    }

    #[tokio::test]
    async fn redis_source_list_brpop() {
        let key = format!("test-key-{}", random_string(10));
        debug!("Test key name: {}.", key);

        let config = RedisSourceConfig {
            data_type: Type::List,
            url: REDIS_SERVER.to_owned(),
            key: key.clone(),
            method: Option::from(Method::BRPOP),
            redis_key: None,
        };

        debug!("Sending event.");
        send_event_by_list(key.clone(), "test message for list(brpop)").await;

        debug!("Receiving event.");
        let (tx, rx) = Pipeline::new_test();
        tokio::spawn(redis_source(&config, ShutdownSignal::noop(), tx).unwrap());
        let events = collect_n(rx, 1).await;

        assert_eq!(
            events[0].as_log()[log_schema().message_key()],
            "test message for list(brpop)".into()
        );
        assert_eq!(
            events[0].as_log()[log_schema().source_type_key()],
            "redis".into()
        );
    }

    #[tokio::test]
    async fn redis_source_list_blpop() {
        let key = format!("test-key-{}", random_string(10));
        debug!("Test key name: {}.", key);

        let config = RedisSourceConfig {
            data_type: Type::List,
            url: REDIS_SERVER.to_owned(),
            key: key.clone(),
            method: Option::from(Method::BLPOP),
            redis_key: None,
        };

        debug!("Sending event.");
        send_event_by_list(key.clone(), "test message for list(blpop)").await;

        debug!("Receiving event.");
        let (tx, rx) = Pipeline::new_test();
        tokio::spawn(redis_source(&config, ShutdownSignal::noop(), tx).unwrap());
        let events = collect_n(rx, 1).await;

        assert_eq!(
            events[0].as_log()[log_schema().message_key()],
            "test message for list(blpop)".into()
        );
        assert_eq!(
            events[0].as_log()[log_schema().source_type_key()],
            "redis".into()
        );
    }

    #[tokio::test]
    async fn redis_source_channel_consume_event() {
        let key = "test-channel".to_owned();
        debug!("Test key name: {}.", key);

        let config = RedisSourceConfig {
            data_type: Type::Channel,
            method: None,
            url: REDIS_SERVER.to_owned(),
            key: key.clone(),
            redis_key: None,
        };

        debug!("Receiving event.");
        let (tx, rx) = Pipeline::new_test();
        tokio::spawn(redis_source(&config, ShutdownSignal::noop(), tx).unwrap());
        let events = collect_n(rx, 1).await;

        assert_eq!(
            events[0].as_log()[log_schema().message_key()],
            "test message for channel".into()
        );
        assert_eq!(
            events[0].as_log()[log_schema().source_type_key()],
            "redis".into()
        );
    }

    #[tokio::test]
    async fn redis_source_channel_product_event() {
        debug!("Sending event by channel.");
        let key = "test-channel".to_owned();
        debug!("Test key name: {}.", key);
        let client = redis::Client::open(REDIS_SERVER).unwrap();
        trace!("Get redis async connection.");
        let mut conn = client
            .get_async_connection()
            .await
            .expect("Failed to get redis async connection.");
        trace!("Get redis async connection success.");
        let res: RedisResult<i32> = conn.publish(key, "test message for channel").await;
        match res {
            Ok(len) => {
                debug!("Send event by channel success, len: {:?}.", len);
                assert_eq!(len, 1);
            }
            Err(err) => {
                panic!("Send event by channel error: {:?}.", err);
            }
        }
    }
}
