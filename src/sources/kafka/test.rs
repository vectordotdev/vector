use vector_lib::{lookup::OwnedTargetPath, schema::Definition};

use super::*;

pub fn kafka_host() -> String {
    std::env::var("KAFKA_HOST").unwrap_or_else(|_| "localhost".into())
}
pub fn kafka_port() -> u16 {
    let port = std::env::var("KAFKA_PORT").unwrap_or_else(|_| "9091".into());
    port.parse().expect("Invalid port number")
}

pub fn kafka_address() -> String {
    format!("{}:{}", kafka_host(), kafka_port())
}

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<KafkaSourceConfig>();
}

#[test]
fn parses_decompression_config() {
    let config: KafkaSourceConfig = toml::from_str(
        r#"
        bootstrap_servers = "localhost:9092"
        topics = ["topic"]
        group_id = "group"

        [decompression]
        algorithm = "zstd"
        dictionary_path = "/etc/vector/compression.dict"
        "#,
    )
    .unwrap();

    let decompression = config.decompression.expect("decompression should be set");
    assert_eq!(
        decompression.algorithm,
        vector_lib::codecs::DecompressionAlgorithm::Zstd
    );
    assert_eq!(
        decompression.dictionary_path,
        Some(std::path::PathBuf::from("/etc/vector/compression.dict"))
    );
}

#[test]
fn decompression_config_is_optional() {
    let config: KafkaSourceConfig = toml::from_str(
        r#"
        bootstrap_servers = "localhost:9092"
        topics = ["topic"]
        group_id = "group"
        "#,
    )
    .unwrap();
    assert!(config.decompression.is_none());
}

pub(super) fn make_config(
    topic: &str,
    group: &str,
    log_namespace: LogNamespace,
    librdkafka_options: Option<HashMap<String, String>>,
) -> KafkaSourceConfig {
    KafkaSourceConfig {
        bootstrap_servers: kafka_address(),
        topics: vec![topic.into()],
        group_id: group.into(),
        auto_offset_reset: "beginning".into(),
        session_timeout_ms: Duration::from_millis(6000),
        commit_interval_ms: Duration::from_millis(1),
        librdkafka_options,
        key_field: default_key_field(),
        topic_key: default_topic_key(),
        partition_key: default_partition_key(),
        offset_key: default_offset_key(),
        headers_key: default_headers_key(),
        socket_timeout_ms: Duration::from_millis(60000),
        fetch_wait_max_ms: Duration::from_millis(100),
        log_namespace: Some(log_namespace == LogNamespace::Vector),
        ..Default::default()
    }
}

#[test]
fn test_output_schema_definition_vector_namespace() {
    let definitions = make_config("topic", "group", LogNamespace::Vector, None)
        .outputs(LogNamespace::Vector)
        .remove(0)
        .schema_definition(true);

    assert_eq!(
        definitions,
        Some(
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                .with_meaning(OwnedTargetPath::event_root(), "message")
                .with_metadata_field(
                    &owned_value_path!("kafka", "timestamp"),
                    Kind::timestamp(),
                    Some("timestamp")
                )
                .with_metadata_field(
                    &owned_value_path!("kafka", "message_key"),
                    Kind::bytes(),
                    None
                )
                .with_metadata_field(&owned_value_path!("kafka", "topic"), Kind::bytes(), None)
                .with_metadata_field(
                    &owned_value_path!("kafka", "partition"),
                    Kind::bytes(),
                    None
                )
                .with_metadata_field(&owned_value_path!("kafka", "offset"), Kind::bytes(), None)
                .with_metadata_field(
                    &owned_value_path!("kafka", "headers"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())),
                    None
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None
                )
        )
    )
}

#[test]
fn test_output_schema_definition_legacy_namespace() {
    let definitions = make_config("topic", "group", LogNamespace::Legacy, None)
        .outputs(LogNamespace::Legacy)
        .remove(0)
        .schema_definition(true);

    assert_eq!(
        definitions,
        Some(
            Definition::new_with_default_metadata(Kind::json(), [LogNamespace::Legacy])
                .unknown_fields(Kind::undefined())
                .with_event_field(
                    &owned_value_path!("message"),
                    Kind::bytes(),
                    Some("message")
                )
                .with_event_field(
                    &owned_value_path!("timestamp"),
                    Kind::timestamp(),
                    Some("timestamp")
                )
                .with_event_field(&owned_value_path!("message_key"), Kind::bytes(), None)
                .with_event_field(&owned_value_path!("topic"), Kind::bytes(), None)
                .with_event_field(&owned_value_path!("partition"), Kind::bytes(), None)
                .with_event_field(&owned_value_path!("offset"), Kind::bytes(), None)
                .with_event_field(
                    &owned_value_path!("headers"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())),
                    None
                )
                .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
        )
    )
}

#[tokio::test]
async fn consumer_create_ok() {
    let config = make_config("topic", "group", LogNamespace::Legacy, None);
    assert!(create_consumer(&config, true).is_ok());
}

#[tokio::test]
async fn consumer_create_incorrect_auto_offset_reset() {
    let config = KafkaSourceConfig {
        auto_offset_reset: "incorrect-auto-offset-reset".to_string(),
        ..make_config("topic", "group", LogNamespace::Legacy, None)
    };
    assert!(create_consumer(&config, true).is_err());
}
