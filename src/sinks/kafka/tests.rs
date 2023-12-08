#![allow(clippy::print_stdout)] // tests

#[cfg(feature = "kafka-integration-tests")]
#[cfg(test)]
mod integration_test {
    use std::{collections::HashMap, future::ready, thread, time::Duration};

    use bytes::Bytes;
    use futures::StreamExt;
    use rdkafka::{
        consumer::{BaseConsumer, Consumer},
        message::Headers,
        Message, Offset, TopicPartitionList,
    };
    use vector_lib::codecs::TextSerializerConfig;
    use vector_lib::lookup::lookup_v2::ConfigTargetPath;
    use vector_lib::{
        config::{init_telemetry, Tags, Telemetry},
        event::{BatchNotifier, BatchStatus},
    };

    use super::super::{
        config::{KafkaRole, KafkaSinkConfig},
        sink::KafkaSink,
        *,
    };
    use crate::{
        event::{ObjectMap, Value},
        kafka::{KafkaAuthConfig, KafkaCompression, KafkaSaslConfig},
        sinks::prelude::*,
        test_util::{
            components::{
                assert_data_volume_sink_compliance, assert_sink_compliance, DATA_VOLUME_SINK_TAGS,
                SINK_TAGS,
            },
            random_lines_with_stream, random_string, wait_for,
        },
        tls::{TlsConfig, TlsEnableableConfig, TEST_PEM_INTERMEDIATE_CA_PATH},
    };

    fn kafka_host() -> String {
        std::env::var("KAFKA_HOST").unwrap_or_else(|_| "localhost".into())
    }

    fn kafka_address(port: u16) -> String {
        format!("{}:{}", kafka_host(), port)
    }

    #[tokio::test]
    async fn healthcheck() {
        crate::test_util::trace_init();

        let topic = format!("test-{}", random_string(10));

        let config = KafkaSinkConfig {
            bootstrap_servers: kafka_address(9091),
            topic: Template::try_from(topic.clone()).unwrap(),
            key_field: None,
            encoding: TextSerializerConfig::default().into(),
            batch: BatchConfig::default(),
            compression: KafkaCompression::None,
            auth: KafkaAuthConfig::default(),
            socket_timeout_ms: Duration::from_millis(60000),
            message_timeout_ms: Duration::from_millis(300000),
            librdkafka_options: HashMap::new(),
            headers_key: None,
            acknowledgements: Default::default(),
        };
        self::sink::healthcheck(config).await.unwrap();
    }

    #[tokio::test]
    async fn kafka_happy_path_plaintext() {
        crate::test_util::trace_init();
        kafka_happy_path(
            kafka_address(9091),
            None,
            None,
            KafkaCompression::None,
            true,
        )
        .await;
        kafka_happy_path(
            kafka_address(9091),
            None,
            None,
            KafkaCompression::None,
            false,
        )
        .await;
    }

    #[tokio::test]
    async fn kafka_happy_path_gzip() {
        crate::test_util::trace_init();
        kafka_happy_path(
            kafka_address(9091),
            None,
            None,
            KafkaCompression::Gzip,
            false,
        )
        .await;
    }

    #[tokio::test]
    async fn kafka_happy_path_lz4() {
        crate::test_util::trace_init();
        kafka_happy_path(
            kafka_address(9091),
            None,
            None,
            KafkaCompression::Lz4,
            false,
        )
        .await;
    }

    #[tokio::test]
    async fn kafka_happy_path_snappy() {
        crate::test_util::trace_init();
        kafka_happy_path(
            kafka_address(9091),
            None,
            None,
            KafkaCompression::Snappy,
            false,
        )
        .await;
    }

    #[tokio::test]
    async fn kafka_happy_path_zstd() {
        crate::test_util::trace_init();
        kafka_happy_path(
            kafka_address(9091),
            None,
            None,
            KafkaCompression::Zstd,
            false,
        )
        .await;
    }

    async fn kafka_batch_options_overrides(
        batch: BatchConfig<NoDefaultsBatchSettings>,
        librdkafka_options: HashMap<String, String>,
    ) -> crate::Result<KafkaSink> {
        let topic = format!("test-{}", random_string(10));
        let config = KafkaSinkConfig {
            bootstrap_servers: kafka_address(9091),
            topic: Template::try_from(format!("{}-%Y%m%d", topic)).unwrap(),
            compression: KafkaCompression::None,
            encoding: TextSerializerConfig::default().into(),
            key_field: None,
            auth: KafkaAuthConfig {
                sasl: None,
                tls: None,
            },
            socket_timeout_ms: Duration::from_millis(60000),
            message_timeout_ms: Duration::from_millis(300000),
            batch,
            librdkafka_options,
            headers_key: None,
            acknowledgements: Default::default(),
        };
        config.clone().to_rdkafka(KafkaRole::Consumer)?;
        config.clone().to_rdkafka(KafkaRole::Producer)?;
        self::sink::healthcheck(config.clone()).await?;
        KafkaSink::new(config)
    }

    #[tokio::test]
    async fn kafka_batch_options_max_bytes_errors_on_double_set() {
        crate::test_util::trace_init();
        let mut batch = BatchConfig::default();
        batch.max_bytes = Some(1000);

        assert!(kafka_batch_options_overrides(
            batch,
            indexmap::indexmap! {
                "batch.size".to_string() => 1.to_string(),
            }
            .into_iter()
            .collect()
        )
        .await
        .is_err())
    }

    #[tokio::test]
    async fn kafka_batch_options_actually_sets() {
        crate::test_util::trace_init();
        let mut batch = BatchConfig::default();
        batch.max_events = Some(10);
        batch.timeout_secs = Some(2.0);

        kafka_batch_options_overrides(batch, indexmap::indexmap! {}.into_iter().collect())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn kafka_batch_options_max_events_errors_on_double_set() {
        crate::test_util::trace_init();
        let mut batch = BatchConfig::default();
        batch.max_events = Some(10);

        assert!(kafka_batch_options_overrides(
            batch,
            indexmap::indexmap! {
                "batch.num.messages".to_string() => 1.to_string(),
            }
            .into_iter()
            .collect()
        )
        .await
        .is_err())
    }

    #[tokio::test]
    async fn kafka_batch_options_timeout_secs_errors_on_double_set() {
        crate::test_util::trace_init();
        let mut batch = BatchConfig::default();
        batch.timeout_secs = Some(10.0);

        assert!(kafka_batch_options_overrides(
            batch,
            indexmap::indexmap! {
                "queue.buffering.max.ms".to_string() => 1.to_string(),
            }
            .into_iter()
            .collect()
        )
        .await
        .is_err())
    }

    #[tokio::test]
    async fn kafka_happy_path_tls() {
        crate::test_util::trace_init();
        let mut options = TlsConfig::test_config();
        // couldn't get Kafka to load and return a certificate chain, it only returns the leaf
        // certificate
        options.ca_file = Some(TEST_PEM_INTERMEDIATE_CA_PATH.into());
        kafka_happy_path(
            kafka_address(9092),
            None,
            Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig::test_config(),
            }),
            KafkaCompression::None,
            false,
        )
        .await;
    }

    #[tokio::test]
    async fn kafka_happy_path_sasl() {
        crate::test_util::trace_init();
        kafka_happy_path(
            kafka_address(9093),
            Some(KafkaSaslConfig {
                enabled: Some(true),
                username: Some("admin".to_string()),
                password: Some("admin".to_string().into()),
                mechanism: Some("PLAIN".to_owned()),
            }),
            None,
            KafkaCompression::None,
            false,
        )
        .await;
    }

    async fn kafka_happy_path(
        server: String,
        sasl: Option<KafkaSaslConfig>,
        tls: Option<TlsEnableableConfig>,
        compression: KafkaCompression,
        test_telemetry_tags: bool,
    ) {
        if test_telemetry_tags {
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
        }

        let topic = format!("test-{}", random_string(10));
        let headers_key = ConfigTargetPath::try_from("headers_key".to_string()).unwrap();
        let kafka_auth = KafkaAuthConfig { sasl, tls };
        let config = KafkaSinkConfig {
            bootstrap_servers: server.clone(),
            topic: Template::try_from(format!("{}-%Y%m%d", topic)).unwrap(),
            key_field: None,
            encoding: TextSerializerConfig::default().into(),
            batch: BatchConfig::default(),
            compression,
            auth: kafka_auth.clone(),
            socket_timeout_ms: Duration::from_millis(60000),
            message_timeout_ms: Duration::from_millis(300000),
            librdkafka_options: HashMap::new(),
            headers_key: Some(headers_key.clone()),
            acknowledgements: Default::default(),
        };
        let topic = format!("{}-{}", topic, chrono::Utc::now().format("%Y%m%d"));
        println!("Topic name generated in test: {:?}", topic);

        let num_events = 1000;
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input, events) = random_lines_with_stream(100, num_events, Some(batch));

        let header_1_key = "header-1-key";
        let header_1_value = "header-1-value";
        let input_events = events.map(move |mut events| {
            let headers_key = headers_key.clone();
            let mut header_values = ObjectMap::new();
            header_values.insert(
                header_1_key.into(),
                Value::Bytes(Bytes::from(header_1_value)),
            );
            events.iter_logs_mut().for_each(move |log| {
                log.insert(&headers_key, header_values.clone());
            });
            events
        });

        if test_telemetry_tags {
            assert_data_volume_sink_compliance(&DATA_VOLUME_SINK_TAGS, async move {
                let sink = KafkaSink::new(config).unwrap();
                let sink = VectorSink::from_event_streamsink(sink);
                sink.run(input_events).await
            })
            .await
            .expect("Running sink failed");
        } else {
            assert_sink_compliance(&SINK_TAGS, async move {
                let sink = KafkaSink::new(config).unwrap();
                let sink = VectorSink::from_event_streamsink(sink);
                sink.run(input_events).await
            })
            .await
            .expect("Running sink failed");
        }
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        // read back everything from the beginning
        let mut client_config = rdkafka::ClientConfig::new();
        client_config.set("bootstrap.servers", server.as_str());
        client_config.set("group.id", &random_string(10));
        client_config.set("enable.partition.eof", "true");
        kafka_auth.apply(&mut client_config).unwrap();

        let mut tpl = TopicPartitionList::new();
        tpl.add_partition(&topic, 0)
            .set_offset(Offset::Beginning)
            .unwrap();

        let consumer: BaseConsumer = client_config.create().unwrap();
        consumer.assign(&tpl).unwrap();

        // wait for messages to show up
        wait_for(
            || match consumer.fetch_watermarks(&topic, 0, Duration::from_secs(3)) {
                Ok((_low, high)) => ready(high > 0),
                Err(err) => {
                    println!("retrying due to error fetching watermarks: {}", err);
                    ready(false)
                }
            },
        )
        .await;

        // check we have the expected number of messages in the topic
        let (low, high) = consumer
            .fetch_watermarks(&topic, 0, Duration::from_secs(3))
            .unwrap();
        assert_eq!((0, num_events as i64), (low, high));

        // loop instead of iter so we can set a timeout
        let mut failures = 0;
        let mut out = Vec::new();
        while failures < 100 {
            match consumer.poll(Duration::from_secs(3)) {
                Some(Ok(msg)) => {
                    let s: &str = msg.payload_view().unwrap().unwrap();
                    out.push(s.to_owned());
                    let header = msg.headers().unwrap().get(0);
                    assert_eq!(header.key, header_1_key);
                    assert_eq!(header.value.unwrap(), header_1_value.as_bytes());
                }
                None if out.len() >= input.len() => break,
                _ => {
                    failures += 1;
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }

        assert_eq!(out.len(), input.len());
        assert_eq!(out, input);
    }
}
