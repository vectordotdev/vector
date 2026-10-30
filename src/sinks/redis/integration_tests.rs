use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use futures::stream;
use rand::RngExt;
use redis::AsyncCommands;
use vector_lib::{
    codecs::{JsonSerializerConfig, TextSerializerConfig},
    config::{Tags, Telemetry, init_telemetry},
    event::LogEvent,
    stream::Driver,
};

use super::{
    RedisEvent, RedisRequest,
    config::{
        DataTypeConfig, ListMethod, ListOption, RedisSinkConfig, RedisTowerRequestConfigDefaults,
        SortedSetMethod, SortedSetOption,
    },
    request_builder::request_builder,
    service::{RedisResponse, RedisService},
    sink::{RedisConnection, RedisRetryLogic},
};
use crate::{
    event::{BatchNotifier, BatchStatus, Event, Metric, MetricKind, MetricValue, TraceEvent},
    metrics::{self, Controller},
    serde::OneOrMany,
    sinks::{prelude::*, util::retries::JitterMode},
    template::Template,
    test_util::{
        components::{
            DATA_VOLUME_SINK_TAGS, SINK_TAGS, assert_data_volume_sink_compliance,
            assert_sink_compliance,
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

#[derive(Clone, Copy, Debug)]
enum SubscriptionChange {
    SubscribeOther,
    Unsubscribe,
}

#[tokio::test]
async fn redis_channel_delivery_accounting_preserves_retries() {
    assert_channel_retry(
        SubscriptionChange::SubscribeOther,
        BatchStatus::Errored,
        0.0,
    )
    .await;
}

#[tokio::test]
async fn redis_channel_retry_preserves_delivery_after_unsubscribe() {
    assert_channel_retry(SubscriptionChange::Unsubscribe, BatchStatus::Errored, 1.0).await;
}

async fn assert_channel_retry(
    change: SubscriptionChange,
    expected_status: BatchStatus,
    expected_discarded: f64,
) {
    metrics::init_test();
    let controller = Controller::get().unwrap();
    controller.reset();
    let client = redis::Client::open(redis_server()).unwrap();
    let channel_prefix = format!("vector-retry-{}", random_string(10));
    let connection = RedisConnection::new_direct(client.get_connection_manager().await.unwrap());
    let service = RedisService {
        conn: connection.clone(),
        data_type: super::DataType::Channel,
    };
    let first_channel = format!("{channel_prefix}-first");
    let second_channel = format!("{channel_prefix}-second");
    let mut subscriber = client.get_async_pubsub().await.unwrap();
    subscriber.subscribe(&first_channel).await.unwrap();
    let subscriber = Arc::new(tokio::sync::Mutex::new(subscriber));
    let attempts = Arc::new(AtomicUsize::new(0));
    let mut settings = TowerRequestConfig::<RedisTowerRequestConfigDefaults> {
        retry_attempts: 2,
        timeout_secs: 2,
        retry_jitter_mode: JitterMode::None,
        ..Default::default()
    }
    .into_settings();
    settings.retry_initial_backoff = Duration::from_millis(1);
    settings.retry_max_duration = Duration::from_millis(1);
    let service = ServiceBuilder::new()
        .settings(settings, RedisRetryLogic::new(connection))
        .service(channel_retry_service(
            service,
            change,
            Arc::clone(&subscriber),
            Arc::clone(&attempts),
            channel_prefix.clone(),
        ));
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let events = [("first", first_channel), ("second", second_channel)]
        .into_iter()
        .map(|(message, key)| RedisEvent {
            event: LogEvent::from(message).with_batch_notifier(&batch).into(),
            key,
            score: None,
        })
        .collect();
    drop(batch);
    let request = request_builder(
        events,
        &Transformer::default(),
        &mut Encoder::<()>::new(TextSerializerConfig::default().build().into()),
    );
    Driver::new(stream::iter([request]), service)
        .run()
        .await
        .unwrap();
    if matches!(change, SubscriptionChange::SubscribeOther) {
        let mut subscriber = subscriber.lock().await;
        let message = subscriber.on_message().next().await.unwrap();
        assert_eq!(message.get_payload::<String>().unwrap(), "second");
    }
    assert_eq!(attempts.load(Ordering::Relaxed), 3);
    let delivered = expected_status == BatchStatus::Delivered;
    assert_redis_metrics(
        &format!("{change:?}"),
        expected_discarded,
        if delivered { 0.0 } else { 1.0 },
        if delivered { 2.0 } else { 0.0 },
    );
    assert_eq!(receiver.await, expected_status, "{change:?}");
}

fn channel_retry_service(
    service: RedisService,
    change: SubscriptionChange,
    subscriber: Arc<tokio::sync::Mutex<redis::aio::PubSub>>,
    attempts: Arc<AtomicUsize>,
    channel_prefix: String,
) -> impl Service<RedisRequest, Response = RedisResponse, Error = super::RedisSinkError, Future: Send>
+ Clone {
    tower::service_fn(move |request| {
        let attempt = attempts.fetch_add(1, Ordering::Relaxed);
        let mut service = service.clone();
        let subscriber = Arc::clone(&subscriber);
        let channel_prefix = channel_prefix.clone();
        async move {
            let result = service.call(request).await;
            if attempt == 0 {
                assert_eq!(result.as_ref().unwrap().event_status, [true, false]);
                let mut subscriber = subscriber.lock().await;
                let message = subscriber.on_message().next().await.unwrap();
                assert_eq!(message.get_payload::<String>().unwrap(), "first");
                subscriber
                    .unsubscribe(format!("{channel_prefix}-first"))
                    .await
                    .unwrap();
                match change {
                    SubscriptionChange::SubscribeOther => {
                        subscriber
                            .subscribe(format!("{channel_prefix}-second"))
                            .await
                            .unwrap();
                    }
                    SubscriptionChange::Unsubscribe => {}
                }
            }
            result
        }
    })
}

#[tokio::test]
async fn redis_sink_terminal_outcome_metrics() {
    use BatchStatus::{Delivered, Errored};
    use DataTypeConfig::{Channel, List, SortedSet};
    use ListMethod::{LPush, RPush};

    metrics::init_test();
    for (name, data_type, list_method, messages, expected_status) in [
        (
            "sorted-set-mixed",
            SortedSet,
            None,
            vec!["new", "existing"],
            Delivered,
        ),
        (
            "sorted-set-existing",
            SortedSet,
            None,
            vec!["existing"],
            Delivered,
        ),
        (
            "list-lpush",
            List,
            Some(LPush),
            vec!["first", "second"],
            Delivered,
        ),
        (
            "list-rpush",
            List,
            Some(RPush),
            vec!["first", "second"],
            Delivered,
        ),
        (
            "channel-no-subscribers",
            Channel,
            None,
            vec!["first", "second"],
            Errored,
        ),
    ] {
        assert_redis_sink_outcome(name, data_type, list_method, &messages, expected_status).await;
    }
}

async fn assert_redis_sink_outcome(
    name: &str,
    data_type: DataTypeConfig,
    list_method: Option<ListMethod>,
    messages: &[&str],
    expected_status: BatchStatus,
) {
    let controller = Controller::get().unwrap();
    controller.reset();
    let key = format!("test-{name}-{}", random_string(10));
    let mut config = RedisSinkConfig {
        endpoint: OneOrMany::One(redis_server()),
        key: Template::try_from(key.clone()).unwrap(),
        encoding: TextSerializerConfig::default().into(),
        data_type,
        list_option: list_method.map(|method| ListOption { method }),
        sorted_set_option: Some(SortedSetOption {
            method: Some(SortedSetMethod::ZAdd),
            score: Some(UnsignedIntTemplate::try_from("2").unwrap()),
        }),
        batch: BatchConfig::default(),
        request: TowerRequestConfig {
            retry_attempts: 1,
            timeout_secs: 2,
            ..Default::default()
        },
        sentinel_service: None,
        sentinel_connect: None,
        acknowledgements: AcknowledgementsConfig::default(),
        confinement: crate::template::ConfinementConfig::default(),
    };
    config.batch.max_events = Some(messages.len());

    let mut conn = config
        .build_connection()
        .await
        .unwrap()
        .get_connection_manager()
        .await
        .unwrap()
        .connection;
    if matches!(data_type, DataTypeConfig::SortedSet) {
        let added: usize = conn.zadd(&key, "existing", 1).await.unwrap();
        assert_eq!(added, 1);
    }

    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let events = messages
        .iter()
        .map(|message| LogEvent::from(*message).with_batch_notifier(&batch).into())
        .collect::<Vec<Event>>();
    drop(batch);
    let (sink, _) = config.build(SinkContext::default()).await.unwrap();
    sink.run_events(events).await.unwrap();

    match data_type {
        DataTypeConfig::SortedSet => {
            let members: Vec<(String, f64)> = conn.zrange_withscores(&key, 0, -1).await.unwrap();
            let mut expected: Vec<_> = messages
                .iter()
                .map(|message| (message.to_string(), 2.0))
                .collect();
            expected.sort_by(|left, right| left.0.cmp(&right.0));
            assert_eq!(members, expected, "{name}");
        }
        DataTypeConfig::List => {
            let length: usize = conn.llen(&key).await.unwrap();
            assert_eq!(length, messages.len(), "{name}");
        }
        DataTypeConfig::Channel => {}
    }
    let _: usize = conn.del(&key).await.unwrap();

    let delivered = expected_status == BatchStatus::Delivered;
    let event_count = f64::from(u32::try_from(messages.len()).unwrap());
    assert_redis_metrics(
        name,
        if delivered { 0.0 } else { event_count },
        if delivered { 0.0 } else { 1.0 },
        if delivered { event_count } else { 0.0 },
    );
    assert_eq!(receiver.await, expected_status, "{name}");
}

fn assert_redis_metrics(name: &str, discarded: f64, errors: f64, sent: f64) {
    let metrics = Controller::get().unwrap().capture_metrics();
    for (metric_name, expected) in [
        ("component_discarded_events_total", discarded),
        ("component_errors_total", errors),
        ("component_sent_events_total", sent),
    ] {
        let count: f64 = metrics
            .iter()
            .filter(|metric| metric.name() == metric_name)
            .map(|metric| match metric.value() {
                MetricValue::Counter { value } => *value,
                _ => panic!("{metric_name} must be a counter"),
            })
            .sum();
        assert_eq!(
            MetricValue::Counter { value: count },
            MetricValue::Counter { value: expected },
            "{name}: {metric_name}"
        );
    }
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
        acknowledgements: AcknowledgementsConfig::default(),
        confinement: crate::template::ConfinementConfig::default(),
    };

    let mut redis_connection = cnf.build_connection().await.unwrap();
    let mut conn = redis_connection
        .get_connection_manager()
        .await
        .unwrap()
        .connection;

    assert!(
        redis::cmd("PING")
            .query_async::<()>(&mut conn)
            .await
            .is_ok()
    );
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
        acknowledgements: AcknowledgementsConfig::default(),
        confinement: crate::template::ConfinementConfig::default(),
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
        acknowledgements: AcknowledgementsConfig::default(),
        confinement: crate::template::ConfinementConfig::default(),
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
        acknowledgements: AcknowledgementsConfig::default(),
        confinement: crate::template::ConfinementConfig::default(),
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
        acknowledgements: AcknowledgementsConfig::default(),
        confinement: crate::template::ConfinementConfig::default(),
    };

    let mut events: Vec<Event> = Vec::new();
    for i in 0..num_events {
        let s: String = i.to_string();
        let mut e = LogEvent::from(s);
        e.insert(vrl::event_path!("num"), i);
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
        acknowledgements: AcknowledgementsConfig::default(),
        confinement: crate::template::ConfinementConfig::default(),
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
        acknowledgements: AcknowledgementsConfig::default(),
        confinement: crate::template::ConfinementConfig::default(),
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
        acknowledgements: AcknowledgementsConfig::default(),
        confinement: crate::template::ConfinementConfig::default(),
    };

    // Create a mix of counter and gauge metrics
    let mut events: Vec<Event> = Vec::new();
    for i in 0..num_events {
        let metric = if i % 2 == 0 {
            // Counter metrics
            Metric::new(
                format!("counter_{i}"),
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: f64::from(u32::try_from(i).expect("test index fits in u32")),
                },
            )
        } else {
            // Gauge metrics
            Metric::new(
                format!("gauge_{i}"),
                MetricKind::Absolute,
                MetricValue::Gauge {
                    value: f64::from(u32::try_from(i).expect("test index fits in u32")),
                },
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
            assert_eq!(
                json["counter"]["value"],
                f64::from(u32::try_from(i).expect("test index fits in u32"))
            );
        } else {
            // Gauge metrics
            assert_eq!(json["name"], format!("gauge_{i}"));
            assert_eq!(json["kind"], "absolute");
            assert_eq!(
                json["gauge"]["value"],
                f64::from(u32::try_from(i).expect("test index fits in u32"))
            );
        }

        // Verify that the name matches what we expect
        assert_eq!(json["name"].as_str().unwrap(), original_event.name());
    }
}

#[tokio::test]
async fn redis_sink_traces() {
    use crate::test_util::components::{SINK_TAGS, assert_sink_compliance};

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
            acknowledgements: AcknowledgementsConfig::default(),
            confinement: crate::template::ConfinementConfig::default(),
        };

        // Build the sink
        let cx = SinkContext::default();
        let (sink, _) = config.build(cx).await.unwrap();

        // Create a  trace event
        let mut trace = TraceEvent::default();
        trace.insert(vrl::event_path!("name"), "test_trace");
        trace.insert(vrl::event_path!("service"), "redis_test");

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
