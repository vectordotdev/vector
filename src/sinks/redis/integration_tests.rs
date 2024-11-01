use futures::stream;
use rand::Rng;
use redis::AsyncCommands;
use vector_lib::codecs::JsonSerializerConfig;
use vector_lib::{
    config::{init_telemetry, Tags, Telemetry},
    event::LogEvent,
};

use super::config::{DataTypeConfig, ListOption, Method, RedisSinkConfig};
use crate::{
    sinks::prelude::*,
    test_util::{
        components::{
            assert_data_volume_sink_compliance, assert_sink_compliance, DATA_VOLUME_SINK_TAGS,
            SINK_TAGS,
        },
        random_lines_with_stream, random_string, trace_init,
    },
};

fn redis_server() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/0".to_owned())
}

#[tokio::test]
async fn redis_sink_list_lpush() {
    trace_init();

    let key = Template::try_from(format!("test-{}", random_string(10)))
        .expect("should not fail to create key template");
    debug!("Test key name: {}.", key);
    let mut rng = rand::thread_rng();
    let num_events = rng.gen_range(10000..20000);
    debug!("Test events num: {}.", num_events);

    let cnf = RedisSinkConfig {
        endpoint: redis_server(),
        key: key.clone(),
        encoding: JsonSerializerConfig::default().into(),
        data_type: DataTypeConfig::List,
        list_option: Some(ListOption {
            method: Method::LPush,
        }),
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            rate_limit_num: u64::MAX,
            ..Default::default()
        },
        acknowledgements: Default::default(),
    };

    let mut events: Vec<Event> = Vec::new();
    for i in 0..num_events {
        let s: String = i.to_string();
        let e = LogEvent::from(s);
        events.push(e.into());
    }
    let input = stream::iter(events.clone().into_iter().map(Into::into));

    // Publish events.
    let cnf2 = cnf.clone();
    assert_sink_compliance(&SINK_TAGS, async move {
        // let conn = cnf2.build_client().await.unwrap();
        let cx = SinkContext::default();
        let (sink, _healthcheck) = cnf2.build(cx).await.unwrap();
        sink.run(input).await
    })
    .await
    .expect("Running sink failed");

    let mut conn = cnf.build_client().await.unwrap();

    let key_exists: bool = conn.exists(key.clone().to_string()).await.unwrap();
    debug!("Test key: {} exists: {}.", key, key_exists);
    assert!(key_exists);
    let llen: usize = conn.llen(key.clone().to_string()).await.unwrap();
    debug!("Test key: {} len: {}.", key, llen);
    assert_eq!(llen, num_events);

    for i in 0..num_events {
        let e = events.get(i).unwrap().as_log();
        let s = serde_json::to_string(e).unwrap_or_default();
        let payload: (String, String) = conn.brpop(key.clone().to_string(), 2000.0).await.unwrap();
        let val = payload.1;
        assert_eq!(val, s);
    }
}

#[tokio::test]
async fn redis_sink_list_rpush() {
    trace_init();

    let key = Template::try_from(format!("test-{}", random_string(10)))
        .expect("should not fail to create key template");
    debug!("Test key name: {}.", key);
    let mut rng = rand::thread_rng();
    let num_events = rng.gen_range(10000..20000);
    debug!("Test events num: {}.", num_events);

    let cnf = RedisSinkConfig {
        endpoint: redis_server(),
        key: key.clone(),
        encoding: JsonSerializerConfig::default().into(),
        data_type: DataTypeConfig::List,
        list_option: Some(ListOption {
            method: Method::RPush,
        }),
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            rate_limit_num: u64::MAX,
            ..Default::default()
        },
        acknowledgements: Default::default(),
    };

    let mut events: Vec<Event> = Vec::new();
    for i in 0..num_events {
        let s: String = i.to_string();
        let e = LogEvent::from(s);
        events.push(e.into());
    }
    let input = stream::iter(events.clone().into_iter().map(Into::into));

    // Publish events.
    let cnf2 = cnf.clone();
    assert_sink_compliance(&SINK_TAGS, async move {
        // let conn = cnf2.build_client().await.unwrap();
        let cx = SinkContext::default();
        let (sink, _healthcheck) = cnf2.build(cx).await.unwrap();
        sink.run(input).await
    })
    .await
    .expect("Running sink failed");

    let mut conn = cnf.build_client().await.unwrap();

    let key_exists: bool = conn.exists(key.to_string()).await.unwrap();
    debug!("Test key: {} exists: {}.", key, key_exists);
    assert!(key_exists);
    let llen: usize = conn.llen(key.clone().to_string()).await.unwrap();
    debug!("Test key: {} len: {}.", key, llen);
    assert_eq!(llen, num_events);

    for i in 0..num_events {
        let e = events.get(i).unwrap().as_log();
        let s = serde_json::to_string(e).unwrap_or_default();
        let payload: (String, String) = conn.blpop(key.clone().to_string(), 2000.0).await.unwrap();
        let val = payload.1;
        assert_eq!(val, s);
    }
}

#[tokio::test]
async fn redis_sink_channel() {
    trace_init();

    let key = Template::try_from(format!("test-{}", random_string(10)))
        .expect("should not fail to create key template");
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
    debug!("Subscribe channel:{}.", key);
    pubsub_conn
        .subscribe(key.clone().to_string())
        .await
        .unwrap_or_else(|_| panic!("Failed to subscribe channel:{}.", key));
    debug!("Subscribed to channel:{}.", key);
    let mut pubsub_stream = pubsub_conn.on_message();

    let cnf = RedisSinkConfig {
        endpoint: redis_server(),
        key: key.clone(),
        encoding: JsonSerializerConfig::default().into(),
        data_type: DataTypeConfig::Channel,
        list_option: None,
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            rate_limit_num: u64::MAX,
            ..Default::default()
        },
        acknowledgements: Default::default(),
    };

    // Publish events.
    assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = cnf.build(cx).await.unwrap(); // Box::new(RedisSink::new(&cnf, conn).unwrap());
        let (_input, events) = random_lines_with_stream(100, num_events, None);
        sink.run(events).await
    })
    .await
    .expect("Running sink failed");

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

#[tokio::test]
async fn redis_sink_channel_data_volume_tags() {
    trace_init();

    // We need to configure Vector to emit the service and source tags.
    // The default is to not emit these.
    init_telemetry(
        Telemetry {
            tags: Tags {
                emit_service: true,
                emit_source: true,
            },
        },
        true,
    );

    let key = Template::try_from(format!("test-{}", random_string(10)))
        .expect("should not fail to create key template");
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
    debug!("Subscribe channel:{}.", key);
    pubsub_conn
        .subscribe(key.clone().to_string())
        .await
        .unwrap_or_else(|_| panic!("Failed to subscribe channel:{}.", key));
    debug!("Subscribed to channel:{}.", key);
    let mut pubsub_stream = pubsub_conn.on_message();

    let cnf = RedisSinkConfig {
        endpoint: redis_server(),
        key: key.clone(),
        encoding: JsonSerializerConfig::default().into(),
        data_type: DataTypeConfig::Channel,
        list_option: None,
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            rate_limit_num: u64::MAX,
            ..Default::default()
        },
        acknowledgements: Default::default(),
    };

    // Publish events.
    assert_data_volume_sink_compliance(&DATA_VOLUME_SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = cnf.build(cx).await.unwrap(); // Box::new(RedisSink::new(&cnf, conn).unwrap());
        let (_input, events) = random_lines_with_stream(100, num_events, None);
        sink.run(events).await
    })
    .await
    .expect("Running sink failed");

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
