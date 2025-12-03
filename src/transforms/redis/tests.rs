#[cfg(feature = "redis-integration-tests")]
use std::sync::Arc;

#[cfg(feature = "redis-integration-tests")]
use tokio::sync::mpsc;
#[cfg(feature = "redis-integration-tests")]
use tokio_stream::wrappers::ReceiverStream;
use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path};

use super::*;
#[cfg(feature = "redis-integration-tests")]
use crate::{
    config::{ComponentKey, OutputId, schema::Definition},
    event::Event,
    test_util::components::assert_transform_compliance,
    transforms::test::create_topology,
};
use crate::{event::LogEvent, template::Template};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<RedisTransformConfig>();
}

#[test]
fn redis_transform_config_creation() {
    let config = RedisTransformConfig {
        url: "redis://127.0.0.1:6379/0".to_string(),
        key: Template::try_from("user:{{ user_id }}").unwrap(),
        output_field: OptionalValuePath::from(owned_value_path!("redis_data")),
        default_value: None,
        cache_max_size: None,
        cache_ttl: None,
    };

    assert_eq!(config.url, "redis://127.0.0.1:6379/0");
}

#[test]
fn redis_transform_with_default_value() {
    let config = RedisTransformConfig {
        url: "redis://127.0.0.1:6379/0".to_string(),
        key: Template::try_from("session:{{ session_id }}").unwrap(),
        output_field: OptionalValuePath::from(owned_value_path!("session_data")),
        default_value: Some("default_session".to_string()),
        cache_max_size: None,
        cache_ttl: None,
    };

    assert_eq!(config.default_value, Some("default_session".to_string()));
}

#[test]
fn redis_transform_key_template_rendering() {
    let template = Template::try_from("user:{{ user_id }}").unwrap();
    let mut log = LogEvent::from("test message");
    log.insert("user_id", "12345");

    let event = crate::event::Event::from(log);
    let rendered = template.render_string(&event).unwrap();
    assert_eq!(rendered, "user:12345");
}

#[test]
fn redis_transform_output_field_path() {
    let output_field = OptionalValuePath::from(owned_value_path!("enrichment", "user_data"));

    assert!(output_field.path.is_some());
    let path = output_field.path.as_ref().unwrap();
    assert_eq!(path.to_string(), "enrichment.user_data");
}

#[cfg(feature = "redis-integration-tests")]
#[tokio::test]
async fn redis_transform_integration() {
    use redis::AsyncCommands;

    const REDIS_SERVER: &str = "redis://127.0.0.1:6379/0";

    assert_transform_compliance(async {
        // Set up test data in Redis
        let client = redis::Client::open(REDIS_SERVER).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();

        let test_key = "test:user:12345";
        let test_value = "John Doe";
        let _: () = conn.set(test_key, test_value).await.unwrap();

        // Create transform config
        let config = RedisTransformConfig {
            url: REDIS_SERVER.to_string(),
            key: Template::try_from("test:user:{{ user_id }}").unwrap(),
            output_field: OptionalValuePath::from(owned_value_path!("user_name")),
            default_value: None,
            cache_max_size: None,
            cache_ttl: None,
        };

        let (tx, rx) = mpsc::channel(1);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        // Create event with user_id
        let mut log = LogEvent::from("test message");
        log.insert("user_id", "12345");
        let mut event = Event::from(log);
        event.set_source_id(Arc::new(ComponentKey::from("in")));
        event.set_upstream_id(Arc::new(OutputId::from("transform")));
        event
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        tx.send(event.clone()).await.unwrap();

        // Wait for transformed event
        let transformed = out.recv().await.unwrap();
        let transformed_log = transformed.as_log();

        // Verify the Redis value was added
        assert_eq!(transformed_log.get("user_name"), Some(&"John Doe".into()));

        // Cleanup
        let _: () = conn.del(test_key).await.unwrap();

        drop(tx);
        topology.stop().await;
        assert_eq!(out.recv().await, None);
    })
    .await;
}

#[cfg(feature = "redis-integration-tests")]
#[tokio::test]
async fn redis_transform_missing_key_with_default() {
    const REDIS_SERVER: &str = "redis://127.0.0.1:6379/0";

    assert_transform_compliance(async {
        let config = RedisTransformConfig {
            url: REDIS_SERVER.to_string(),
            key: Template::try_from("missing:key:{{ user_id }}").unwrap(),
            output_field: OptionalValuePath::from(owned_value_path!("default_field")),
            default_value: Some("not_found".to_string()),
            cache_max_size: None,
            cache_ttl: None,
        };

        let (tx, rx) = mpsc::channel(1);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        let mut log = LogEvent::from("test message");
        log.insert("user_id", "99999");
        let mut event = Event::from(log);
        event.set_source_id(Arc::new(ComponentKey::from("in")));
        event.set_upstream_id(Arc::new(OutputId::from("transform")));
        event
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        tx.send(event.clone()).await.unwrap();

        let transformed = out.recv().await.unwrap();
        let transformed_log = transformed.as_log();

        // Verify default value was used
        assert_eq!(
            transformed_log.get("default_field"),
            Some(&"not_found".into())
        );

        drop(tx);
        topology.stop().await;
        assert_eq!(out.recv().await, None);
    })
    .await;
}

#[cfg(feature = "redis-integration-tests")]
#[tokio::test]
async fn redis_transform_cache() {
    use redis::AsyncCommands;
    use std::num::NonZeroUsize;

    const REDIS_SERVER: &str = "redis://127.0.0.1:6379/0";

    assert_transform_compliance(async {
        // Set up test data in Redis
        let client = redis::Client::open(REDIS_SERVER).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();

        let test_key = "test:cache:user:12345";
        let test_value = "Cached User";
        let _: () = conn.set(test_key, test_value).await.unwrap();

        // Create transform config with cache enabled
        let config = RedisTransformConfig {
            url: REDIS_SERVER.to_string(),
            key: Template::try_from("test:cache:user:{{ user_id }}").unwrap(),
            output_field: OptionalValuePath::from(owned_value_path!("user_name")),
            default_value: None,
            cache_max_size: NonZeroUsize::new(100),
            cache_ttl: None,
        };

        let (tx, rx) = mpsc::channel(10);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        // Create first event - should hit Redis
        let mut log1 = LogEvent::from("test message 1");
        log1.insert("user_id", "12345");
        let mut event1 = Event::from(log1);
        event1.set_source_id(Arc::new(ComponentKey::from("in")));
        event1.set_upstream_id(Arc::new(OutputId::from("transform")));
        event1
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        tx.send(event1.clone()).await.unwrap();
        let transformed1 = out.recv().await.unwrap();
        let transformed_log1 = transformed1.as_log();
        assert_eq!(
            transformed_log1.get("user_name"),
            Some(&"Cached User".into())
        );

        // Delete the key from Redis to verify cache is being used
        let _: () = conn.del(test_key).await.unwrap();

        // Create second event with same user_id - should use cache, not Redis
        let mut log2 = LogEvent::from("test message 2");
        log2.insert("user_id", "12345");
        let mut event2 = Event::from(log2);
        event2.set_source_id(Arc::new(ComponentKey::from("in")));
        event2.set_upstream_id(Arc::new(OutputId::from("transform")));
        event2
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        tx.send(event2.clone()).await.unwrap();
        let transformed2 = out.recv().await.unwrap();
        let transformed_log2 = transformed2.as_log();
        // Should still have the value from cache even though Redis key is deleted
        assert_eq!(
            transformed_log2.get("user_name"),
            Some(&"Cached User".into())
        );

        drop(tx);
        topology.stop().await;
        assert_eq!(out.recv().await, None);
    })
    .await;
}

#[cfg(feature = "redis-integration-tests")]
#[tokio::test]
async fn redis_transform_cache_ttl_expiration() {
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use redis::AsyncCommands;

    const REDIS_SERVER: &str = "redis://127.0.0.1:6379/0";

    assert_transform_compliance(async {
        // Set up test data in Redis
        let client = redis::Client::open(REDIS_SERVER).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();

        let test_key = "test:ttl:user:12345";
        let initial_value = "Initial Value";
        let _: () = conn.set(test_key, initial_value).await.unwrap();

        // Create transform config with cache and short TTL (1 second)
        let config = RedisTransformConfig {
            url: REDIS_SERVER.to_string(),
            key: Template::try_from("test:ttl:user:{{ user_id }}").unwrap(),
            output_field: OptionalValuePath::from(owned_value_path!("user_name")),
            default_value: None,
            cache_max_size: NonZeroUsize::new(100),
            cache_ttl: Some(Duration::from_secs(1)),
        };

        let (tx, rx) = mpsc::channel(10);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        // Create first event - should hit Redis and cache the value
        let mut log1 = LogEvent::from("test message 1");
        log1.insert("user_id", "12345");
        let mut event1 = Event::from(log1);
        event1.set_source_id(Arc::new(ComponentKey::from("in")));
        event1.set_upstream_id(Arc::new(OutputId::from("transform")));
        event1
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        tx.send(event1.clone()).await.unwrap();
        let transformed1 = out.recv().await.unwrap();
        let transformed_log1 = transformed1.as_log();
        assert_eq!(
            transformed_log1.get("user_name"),
            Some(&initial_value.into())
        );

        // Update the value in Redis
        let updated_value = "Updated Value";
        let _: () = conn.set(test_key, updated_value).await.unwrap();

        // Create second event immediately - should use cache (not expired yet)
        let mut log2 = LogEvent::from("test message 2");
        log2.insert("user_id", "12345");
        let mut event2 = Event::from(log2);
        event2.set_source_id(Arc::new(ComponentKey::from("in")));
        event2.set_upstream_id(Arc::new(OutputId::from("transform")));
        event2
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        tx.send(event2.clone()).await.unwrap();
        let transformed2 = out.recv().await.unwrap();
        let transformed_log2 = transformed2.as_log();
        // Should still have the cached value
        assert_eq!(
            transformed_log2.get("user_name"),
            Some(&initial_value.into())
        );

        // Wait for TTL to expire (1 second + small buffer)
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // Create third event after TTL expiration - should refresh from Redis
        let mut log3 = LogEvent::from("test message 3");
        log3.insert("user_id", "12345");
        let mut event3 = Event::from(log3);
        event3.set_source_id(Arc::new(ComponentKey::from("in")));
        event3.set_upstream_id(Arc::new(OutputId::from("transform")));
        event3
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        tx.send(event3.clone()).await.unwrap();
        let transformed3 = out.recv().await.unwrap();
        let transformed_log3 = transformed3.as_log();
        // Should have the updated value from Redis (cache refreshed)
        assert_eq!(
            transformed_log3.get("user_name"),
            Some(&updated_value.into())
        );

        drop(tx);
        topology.stop().await;
        assert_eq!(out.recv().await, None);
    })
    .await;
}

#[cfg(feature = "redis-integration-tests")]
#[tokio::test]
async fn redis_transform_missing_key_no_default() {
    const REDIS_SERVER: &str = "redis://127.0.0.1:6379/0";

    assert_transform_compliance(async {
        let config = RedisTransformConfig {
            url: REDIS_SERVER.to_string(),
            key: Template::try_from("missing:key:{{ user_id }}").unwrap(),
            output_field: OptionalValuePath::from(owned_value_path!("optional_field")),
            default_value: None,
            cache_max_size: None,
            cache_ttl: None,
        };

        let (tx, rx) = mpsc::channel(1);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        let mut log = LogEvent::from("test message");
        log.insert("user_id", "99999");
        let mut event = Event::from(log);
        event.set_source_id(Arc::new(ComponentKey::from("in")));
        event.set_upstream_id(Arc::new(OutputId::from("transform")));
        event
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        tx.send(event.clone()).await.unwrap();

        let transformed = out.recv().await.unwrap();
        let transformed_log = transformed.as_log();

        // Verify field was not added when key is missing and no default
        assert_eq!(transformed_log.get("optional_field"), None);

        drop(tx);
        topology.stop().await;
        assert_eq!(out.recv().await, None);
    })
    .await;
}

#[cfg(feature = "redis-integration-tests")]
#[tokio::test]
async fn redis_transform_metric_event() {
    use crate::event::metric::{Metric, MetricKind, MetricValue};
    use redis::AsyncCommands;

    const REDIS_SERVER: &str = "redis://127.0.0.1:6379/0";

    assert_transform_compliance(async {
        // Set up test data in Redis
        let client = redis::Client::open(REDIS_SERVER).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();

        let test_key = "test:metric:app:network-policies";
        let test_value = "production";
        let _: () = conn.set(test_key, test_value).await.unwrap();

        // Create transform config
        // For metrics, access tags using tags.tag_name syntax
        let config = RedisTransformConfig {
            url: REDIS_SERVER.to_string(),
            key: Template::try_from("test:metric:app:{{ tags.application }}").unwrap(),
            output_field: OptionalValuePath::from(owned_value_path!("environment")),
            default_value: None,
            cache_max_size: None,
            cache_ttl: None,
        };

        let (tx, rx) = mpsc::channel(1);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        // Create metric event with application field
        let mut metric = Metric::new(
            "test_metric",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        );
        metric.replace_tag("application".to_string(), "network-policies".to_string());

        let mut event = Event::Metric(metric);
        event.set_source_id(Arc::new(ComponentKey::from("in")));
        event.set_upstream_id(Arc::new(OutputId::from("transform")));
        event
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        tx.send(event.clone()).await.unwrap();

        // Wait for transformed event
        let transformed = out.recv().await.unwrap();
        let transformed_metric = transformed.as_metric();

        // Verify the Redis value was added as a tag (last segment of path "environment")
        assert_eq!(
            transformed_metric
                .tags()
                .and_then(|tags| tags.get("environment")),
            Some("production")
        );

        // Cleanup
        let _: () = conn.del(test_key).await.unwrap();

        drop(tx);
        topology.stop().await;
        assert_eq!(out.recv().await, None);
    })
    .await;
}

#[cfg(feature = "redis-integration-tests")]
#[tokio::test]
async fn redis_transform_trace_event() {
    use redis::AsyncCommands;
    use vector_lib::event::TraceEvent;
    use vrl::btreemap;

    const REDIS_SERVER: &str = "redis://127.0.0.1:6379/0";

    assert_transform_compliance(async {
        // Set up test data in Redis
        let client = redis::Client::open(REDIS_SERVER).unwrap();
        let mut conn = client.get_connection_manager().await.unwrap();

        let test_key = "test:trace:app:network-policies";
        let test_value = r#"{"team":"platform","tier":"production"}"#;
        let _: () = conn.set(test_key, test_value).await.unwrap();

        // Create transform config
        let config = RedisTransformConfig {
            url: REDIS_SERVER.to_string(),
            key: Template::try_from("test:trace:app:{{ application }}").unwrap(),
            output_field: OptionalValuePath::from(owned_value_path!("app_metadata")),
            default_value: None,
            cache_max_size: None,
            cache_ttl: None,
        };

        let (tx, rx) = mpsc::channel(1);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        // Create trace event with application field
        let trace = TraceEvent::from(btreemap! {
            "application" => "network-policies",
            "span_id" => "abc123",
            "trace_id" => "xyz789",
        });

        let mut event = Event::Trace(trace);
        event.set_source_id(Arc::new(ComponentKey::from("in")));
        event.set_upstream_id(Arc::new(OutputId::from("transform")));
        event
            .metadata_mut()
            .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

        tx.send(event.clone()).await.unwrap();

        // Wait for transformed event
        let transformed = out.recv().await.unwrap();
        let transformed_trace = transformed.as_trace();

        // Verify the Redis value was added to the trace
        assert_eq!(
            transformed_trace.get("app_metadata"),
            Some(&test_value.into())
        );

        // Cleanup
        let _: () = conn.del(test_key).await.unwrap();

        drop(tx);
        topology.stop().await;
        assert_eq!(out.recv().await, None);
    })
    .await;
}
