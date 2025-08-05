use futures::stream;
use rand::Rng;
use redis::AsyncCommands;
use vector_lib::codecs::JsonSerializerConfig;
use vector_lib::{
    config::{init_telemetry, Tags, Telemetry},
    event::LogEvent,
};

use crate::event::{
    BatchNotifier, BatchStatus, Event, Metric, MetricKind, MetricValue, TraceEvent,
};

use super::config::{
    DataTypeConfig, ListMethod, ListOption, RedisSinkConfig, SortedSetMethod, SortedSetOption,
};
use crate::{
    serde::OneOrMany,
    sinks::prelude::*,
    test_util::{
        components::{
            assert_data_volume_sink_compliance, assert_sink_compliance, DATA_VOLUME_SINK_TAGS,
            SINK_TAGS,
        },
        map_event_batch_stream, random_lines_with_stream, random_string, trace_init,
    },
};

fn redis_server() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/0".to_owned())
}

fn redis_sentinel_server() -> Vec<String> {
    vec![std::env::var("SENTINEL_URL").unwrap_or_else(|_| "redis://127.0.0.1:26379/".to_owned())]
}

#[tokio::test]
async fn redis_sink_sentinel_reaches_primary() {
    trace_init();

    let cnf = RedisSinkConfig {
        endpoint: OneOrMany::Many(redis_sentinel_server()),
        key: Template::try_from(format!("test-{}", random_string(10)))
            .expect("should not fail to create key template"),
        encoding: JsonSerializerConfig::default().into(),
        data_type: DataTypeConfig::List,
        list_option: Some(ListOption {
            method: ListMethod::RPush,
        }),
        sorted_set_option: None,
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            rate_limit_num: u64::MAX,
            ..Default::default()
        },
        sentinel_service: Some("vector".to_owned()),
        sentinel_connect: None,
        acknowledgements: Default::default(),
    };

    let mut redis_connection = cnf.build_connection().await.unwrap();
    let mut conn = redis_connection
        .get_connection_manager()
        .await
        .unwrap()
        .connection;

    assert!(redis::cmd("PING")
        .query_async::<()>(&mut conn)
        .await
        .is_ok());
}

#[tokio::test]
async fn redis_sink_sentinel_rpush() {
    trace_init();

    let key = Template::try_from(format!("test-{}", random_string(10)))
        .expect("should not fail to create key template");
    debug!("Test key name: {key}.");
    let mut rng = rand::rng();
    let num_events = rng.random_range(10000..20000);
    debug!("Test events num: {num_events}.");

    let cnf = RedisSinkConfig {
        endpoint: OneOrMany::Many(redis_sentinel_server()),
        key: key.clone(),
        encoding: JsonSerializerConfig::default().into(),
        data_type: DataTypeConfig::List,
        list_option: Some(ListOption {
            method: ListMethod::RPush,
        }),
        sorted_set_option: None,
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            rate_limit_num: u64::MAX,
            ..Default::default()
        },
        sentinel_service: Some("vector".to_owned()),
        sentinel_connect: None,
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
        // let conn = cnf2.build_connection().await.unwrap().get_connection_manager().await.unwrap().connection;
        let cx = SinkContext::default();
        let (sink, _healthcheck) = cnf2.build(cx).await.unwrap();
        sink.run(input).await
    })
    .await
    .expect("Running sink failed");

    let mut conn = cnf
        .build_connection()
        .await
        .unwrap()
        .get_connection_manager()
        .await
        .unwrap()
        .connection;

    let key_exists: bool = conn.exists(key.to_string()).await.unwrap();
    debug!("Test key: {key} exists: {key_exists}.");
    assert!(key_exists);
    let llen: usize = conn.llen(key.clone().to_string()).await.unwrap();
    debug!("Test key: {key} len: {llen}.");
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
async fn redis_sink_list_lpush() {
    trace_init();

    let key = Template::try_from(format!("test-{}", random_string(10)))
        .expect("should not fail to create key template");
    debug!("Test key name: {key}.");
    let mut rng = rand::rng();
    let num_events = rng.random_range(10000..20000);
    debug!("Test events num: {num_events}.");

    let cnf = RedisSinkConfig {
        endpoint: OneOrMany::One(redis_server()),
        key: key.clone(),
        encoding: JsonSerializerConfig::default().into(),
        data_type: DataTypeConfig::List,
        list_option: Some(ListOption {
            method: ListMethod::LPush,
        }),
        sorted_set_option: None,
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            rate_limit_num: u64::MAX,
            ..Default::default()
        },
        sentinel_service: None,
        sentinel_connect: None,
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
        // let conn = cnf2.build_connection().await.unwrap().get_connection_manager().await.unwrap().connection;
        let cx = SinkContext::default();
        let (sink, _healthcheck) = cnf2.build(cx).await.unwrap();
        sink.run(input).await
    })
    .await
    .expect("Running sink failed");

    let mut conn = cnf
        .build_connection()
        .await
        .unwrap()
        .get_connection_manager()
        .await
        .unwrap()
        .connection;

    let key_exists: bool = conn.exists(key.clone().to_string()).await.unwrap();
    debug!("Test key: {key} exists: {key_exists}.");
    assert!(key_exists);
    let llen: usize = conn.llen(key.clone().to_string()).await.unwrap();
    debug!("Test key: {key} len: {llen}.");
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
    debug!("Test key name: {key}.");
    let mut rng = rand::rng();
    let num_events = rng.random_range(10000..20000);
    debug!("Test events num: {num_events}.");

    let cnf = RedisSinkConfig {
        endpoint: OneOrMany::One(redis_server()),
        key: key.clone(),
        encoding: JsonSerializerConfig::default().into(),
        data_type: DataTypeConfig::List,
        list_option: Some(ListOption {
            method: ListMethod::RPush,
        }),
        sorted_set_option: None,
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            rate_limit_num: u64::MAX,
            ..Default::default()
        },
        sentinel_service: None,
        sentinel_connect: None,
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
        // let conn = cnf2.build_connection().await.unwrap().get_connection_manager().await.unwrap().connection;
        let cx = SinkContext::default();
        let (sink, _healthcheck) = cnf2.build(cx).await.unwrap();
        sink.run(input).await
    })
    .await
    .expect("Running sink failed");

    let mut conn = cnf
        .build_connection()
        .await
        .unwrap()
        .get_connection_manager()
        .await
        .unwrap()
        .connection;

    let key_exists: bool = conn.exists(key.to_string()).await.unwrap();
    debug!("Test key: {key} exists: {key_exists}.");
    assert!(key_exists);
    let llen: usize = conn.llen(key.clone().to_string()).await.unwrap();
    debug!("Test key: {key} len: {llen}.");
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
async fn redis_sink_sorted_set_zadd() {
    trace_init();

    let key = Template::try_from(format!("test-{}", random_string(10)))
        .expect("should not fail to create key template");
    debug!("Test key name: {key}.");
    let mut rng = rand::rng();
    let num_events = rng.random_range(10000..20000);
    debug!("Test events num: {num_events}.");

    let cnf = RedisSinkConfig {
        endpoint: OneOrMany::One(redis_server()),
        key: key.clone(),
        encoding: JsonSerializerConfig::default().into(),
        data_type: DataTypeConfig::SortedSet,
        list_option: None,
        sorted_set_option: Some(SortedSetOption {
            method: Some(SortedSetMethod::ZAdd),
            score: Some(UnsignedIntTemplate::try_from("{{ num }}").unwrap()),
        }),
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            rate_limit_num: u64::MAX,
            ..Default::default()
        },
        sentinel_service: None,
        sentinel_connect: None,
        acknowledgements: Default::default(),
    };

    let mut events: Vec<Event> = Vec::new();
    for i in 0..num_events {
        let s: String = i.to_string();
        let mut e = LogEvent::from(s);
        e.insert("num", i);
        events.push(e.into());
    }
    let input = stream::iter(events.clone().into_iter().map(Into::into));

    // Publish events.
    let cnf2 = cnf.clone();
    assert_sink_compliance(&SINK_TAGS, async move {
        // let conn = cnf2.build_connection().await.unwrap().get_connection_manager().await.unwrap().connection;
        let cx = SinkContext::default();
        let (sink, _healthcheck) = cnf2.build(cx).await.unwrap();
        sink.run(input).await
    })
    .await
    .expect("Running sink failed");

    let mut conn = cnf
        .build_connection()
        .await
        .unwrap()
        .get_connection_manager()
        .await
        .unwrap()
        .connection;

    let key_exists: bool = conn.exists(key.clone().to_string()).await.unwrap();
    debug!("Test key: {key} exists: {key_exists}.");
    assert!(key_exists);
    let zcount: usize = conn
        .zcount(key.clone().to_string(), 0, num_events - 1)
        .await
        .unwrap();
    debug!("Test key: {key} count: {zcount}.");
    assert_eq!(zcount, events.len());

    for i in 0..num_events {
        let e = events.get(i).unwrap().as_log();
        let s = serde_json::to_string(e).unwrap_or_default();
        let payload: Vec<String> = conn.zpopmin(key.clone().to_string(), 1).await.unwrap();
        let val = payload.into_iter().next().unwrap();
        assert_eq!(val, s);
    }
}

#[tokio::test]
async fn redis_sink_channel() {
    trace_init();

    let key = Template::try_from(format!("test-{}", random_string(10)))
        .expect("should not fail to create key template");
    debug!("Test key name: {key}.");
    let mut rng = rand::rng();
    let num_events = rng.random_range(10000..20000);
    debug!("Test events num: {num_events}.");

    let client = redis::Client::open(redis_server()).unwrap();
    debug!("Get Redis async connection.");
    let mut pubsub_conn = client
        .get_async_pubsub()
        .await
        .expect("Failed to get Redis async connection.");
    debug!("Get Redis async connection success.");
    debug!("Subscribe channel:{key}.");
    pubsub_conn
        .subscribe(key.clone().to_string())
        .await
        .unwrap_or_else(|_| panic!("Failed to subscribe channel:{key}."));
    debug!("Subscribed to channel:{key}.");
    let mut pubsub_stream = pubsub_conn.on_message();

    let cnf = RedisSinkConfig {
        endpoint: OneOrMany::One(redis_server()),
        key: key.clone(),
        encoding: JsonSerializerConfig::default().into(),
        data_type: DataTypeConfig::Channel,
        list_option: None,
        sorted_set_option: None,
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            rate_limit_num: u64::MAX,
            ..Default::default()
        },
        sentinel_service: None,
        sentinel_connect: None,
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
        debug!("Received msg num:{received_msg_num}.");
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
    debug!("Test key name: {key}.");
    let mut rng = rand::rng();
    let num_events = rng.random_range(10000..20000);
    debug!("Test events num: {num_events}.");

    let client = redis::Client::open(redis_server()).unwrap();
    debug!("Get Redis async connection.");
    let mut pubsub_conn = client
        .get_async_pubsub()
        .await
        .expect("Failed to get Redis async connection.");
    debug!("Get Redis async connection success.");
    debug!("Subscribe channel:{key}.");
    pubsub_conn
        .subscribe(key.clone().to_string())
        .await
        .unwrap_or_else(|_| panic!("Failed to subscribe channel:{key}."));
    debug!("Subscribed to channel:{key}.");
    let mut pubsub_stream = pubsub_conn.on_message();

    let cnf = RedisSinkConfig {
        endpoint: OneOrMany::One(redis_server()),
        key: key.clone(),
        encoding: JsonSerializerConfig::default().into(),
        data_type: DataTypeConfig::Channel,
        list_option: None,
        sorted_set_option: None,
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            rate_limit_num: u64::MAX,
            ..Default::default()
        },
        sentinel_service: None,
        sentinel_connect: None,
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
        debug!("Received msg num:{received_msg_num}.");
        if received_msg_num == num_events {
            assert_eq!(received_msg_num, num_events);
            break;
        }
    }
}

#[tokio::test]
async fn redis_sink_metrics() {
    trace_init();

    let key = Template::try_from(format!("test-metrics-{}", random_string(10)))
        .expect("should not fail to create key template");
    debug!("Test key name: {key}.");
    let num_events = 1000;
    debug!("Test events num: {num_events}.");

    let cnf = RedisSinkConfig {
        endpoint: OneOrMany::One(redis_server()),
        key: key.clone(),
        encoding: JsonSerializerConfig::default().into(),
        data_type: DataTypeConfig::List,
        list_option: Some(ListOption {
            method: ListMethod::RPush,
        }),
        sorted_set_option: None,
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            rate_limit_num: u64::MAX,
            ..Default::default()
        },
        sentinel_service: None,
        sentinel_connect: None,
        acknowledgements: Default::default(),
    };

    // Create a mix of counter and gauge metrics
    let mut events: Vec<Event> = Vec::new();
    for i in 0..num_events {
        let metric = if i % 2 == 0 {
            // Counter metrics
            Metric::new(
                format!("counter_{i}"),
                MetricKind::Absolute,
                MetricValue::Counter { value: i as f64 },
            )
        } else {
            // Gauge metrics
            Metric::new(
                format!("gauge_{i}"),
                MetricKind::Absolute,
                MetricValue::Gauge { value: i as f64 },
            )
        };
        events.push(metric.into());
    }
    let input = stream::iter(events.clone().into_iter().map(Into::into));

    // Publish events
    let cnf2 = cnf.clone();
    assert_sink_compliance(&SINK_TAGS, async move {
        let cx = SinkContext::default();
        let (sink, _healthcheck) = cnf2.build(cx).await.unwrap();
        sink.run(input).await
    })
    .await
    .expect("Running sink failed");

    // Verify metrics were stored correctly
    let mut conn = cnf
        .build_connection()
        .await
        .unwrap()
        .get_connection_manager()
        .await
        .unwrap()
        .connection;

    let key_exists: bool = conn.exists(key.to_string()).await.unwrap();
    debug!("Test key: {key} exists: {key_exists}.");
    assert!(key_exists);

    let llen: usize = conn.llen(key.clone().to_string()).await.unwrap();
    debug!("Test key: {key} len: {llen}.");
    assert_eq!(llen, num_events);

    // Verify the content of each metric
    for i in 0..num_events {
        let original_event = events.get(i).unwrap().as_metric();
        let payload: (String, String) = conn.blpop(key.clone().to_string(), 2000.0).await.unwrap();
        let val = payload.1;

        // Parse the JSON and verify key metric properties
        let json: serde_json::Value = serde_json::from_str(&val).unwrap();

        if i % 2 == 0 {
            // Counter metrics
            assert_eq!(json["name"], format!("counter_{i}"));
            assert_eq!(json["kind"], "absolute");
            assert_eq!(json["counter"]["value"], i as f64);
        } else {
            // Gauge metrics
            assert_eq!(json["name"], format!("gauge_{i}"));
            assert_eq!(json["kind"], "absolute");
            assert_eq!(json["gauge"]["value"], i as f64);
        }

        // Verify that the name matches what we expect
        assert_eq!(json["name"].as_str().unwrap(), original_event.name());
    }
}

#[tokio::test]
async fn redis_sink_traces() {
    use crate::test_util::components::{assert_sink_compliance, SINK_TAGS};

    trace_init();

    assert_sink_compliance(&SINK_TAGS, async {
        // Setup Redis sink config
        let key = Template::try_from(format!("test-traces-{}", random_string(10))).unwrap();
        let config = RedisSinkConfig {
            endpoint: OneOrMany::One(redis_server()),
            key: key.clone(),
            encoding: JsonSerializerConfig::default().into(),
            data_type: DataTypeConfig::List,
            list_option: Some(ListOption {
                method: ListMethod::RPush,
            }),
            sorted_set_option: None,
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            sentinel_service: None,
            sentinel_connect: None,
            acknowledgements: Default::default(),
        };

        // Build the sink
        let cx = SinkContext::default();
        let (sink, _) = config.build(cx).await.unwrap();

        // Create a  trace event
        let mut trace = TraceEvent::default();
        trace.insert("name", "test_trace");
        trace.insert("service", "redis_test");

        // Set up batch notification for checking delivery status
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let trace_with_batch = trace.with_batch_notifier(&batch);

        // Create event stream with proper conversion to EventArray
        let events = vec![Event::Trace(trace_with_batch)];
        let stream = map_event_batch_stream(stream::iter(events), Some(batch));

        // Run the sink with the stream
        sink.run(stream).await.unwrap();

        // Check that events were delivered
        assert_eq!(receiver.await, BatchStatus::Delivered);

        // Verify data in Redis
        let mut conn = redis::Client::open(redis_server())
            .unwrap()
            .get_multiplexed_async_connection()
            .await
            .unwrap();

        let len: usize = conn.llen(key.to_string()).await.unwrap();
        assert_eq!(len, 1);

        // Check content
        let payload: (String, String) = conn.blpop(key.to_string(), 2000.0).await.unwrap();
        let json_str = payload.1;

        // Verify the trace content
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json["name"], "test_trace");
        assert_eq!(json["service"], "redis_test");
    })
    .await;
}
