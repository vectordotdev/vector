#![allow(clippy::print_stdout)] // tests

#[cfg(feature = "kafka-integration-tests")]
#[cfg(test)]
mod integration_test {
    use std::{
        collections::{BTreeMap, HashMap},
        future::ready,
        thread,
        time::Duration,
    };

    use bytes::Bytes;
    use futures::StreamExt;
    use rdkafka::{
        consumer::{BaseConsumer, Consumer},
        message::Headers,
        Message, Offset, TopicPartitionList,
    };
    use vector_core::{
        buffers::Acker,
        event::{BatchNotifier, BatchStatus},
    };

    use crate::{
        event::Value,
        kafka::{KafkaAuthConfig, KafkaCompression, KafkaSaslConfig, KafkaTlsConfig},
        sinks::{
            kafka::{
                config::{KafkaRole, KafkaSinkConfig},
                sink::KafkaSink,
                *,
            },
            util::{
                encoding::{EncodingConfig, StandardEncodings},
                BatchConfig, NoDefaultsBatchSettings,
            },
            VectorSink,
        },
        test_util::{components, random_lines_with_stream, random_string, wait_for},
        tls::TlsOptions,
    };

    #[tokio::test]
    async fn healthcheck() {
        crate::test_util::trace_init();
        let topic = format!("test-{}", random_string(10));

        let config = KafkaSinkConfig {
            bootstrap_servers: "localhost:9091".into(),
            topic: topic.clone(),
            key_field: None,
            encoding: EncodingConfig::from(StandardEncodings::Text),
            batch: BatchConfig::default(),
            compression: KafkaCompression::None,
            auth: KafkaAuthConfig::default(),
            socket_timeout_ms: 60000,
            message_timeout_ms: 300000,
            librdkafka_options: HashMap::new(),
            headers_key: None,
        };

        self::sink::healthcheck(config).await.unwrap();
    }

    #[tokio::test]
    async fn kafka_happy_path_plaintext() {
        crate::test_util::trace_init();
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::None).await;
    }

    #[tokio::test]
    async fn kafka_happy_path_gzip() {
        crate::test_util::trace_init();
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Gzip).await;
    }

    #[tokio::test]
    async fn kafka_happy_path_lz4() {
        crate::test_util::trace_init();
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Lz4).await;
    }

    #[tokio::test]
    async fn kafka_happy_path_snappy() {
        crate::test_util::trace_init();
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Snappy).await;
    }

    #[tokio::test]
    async fn kafka_happy_path_zstd() {
        crate::test_util::trace_init();
        kafka_happy_path("localhost:9091", None, None, KafkaCompression::Zstd).await;
    }

    async fn kafka_batch_options_overrides(
        batch: BatchConfig<NoDefaultsBatchSettings>,
        librdkafka_options: HashMap<String, String>,
    ) -> crate::Result<KafkaSink> {
        let topic = format!("test-{}", random_string(10));
        let config = KafkaSinkConfig {
            bootstrap_servers: "localhost:9091".to_string(),
            topic: format!("{}-%Y%m%d", topic),
            compression: KafkaCompression::None,
            encoding: StandardEncodings::Text.into(),
            key_field: None,
            auth: KafkaAuthConfig {
                sasl: None,
                tls: None,
            },
            socket_timeout_ms: 60000,
            message_timeout_ms: 300000,
            batch,
            librdkafka_options,
            headers_key: None,
        };
        let (acker, _ack_counter) = Acker::basic();
        config.clone().to_rdkafka(KafkaRole::Consumer)?;
        config.clone().to_rdkafka(KafkaRole::Producer)?;
        self::sink::healthcheck(config.clone()).await?;
        KafkaSink::new(config, acker)
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
        batch.timeout_secs = Some(2);

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
        batch.timeout_secs = Some(10);

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
        kafka_happy_path(
            "localhost:9092",
            None,
            Some(KafkaTlsConfig {
                enabled: Some(true),
                options: TlsOptions::test_options(),
            }),
            KafkaCompression::None,
        )
        .await;
    }

    #[tokio::test]
    async fn kafka_happy_path_tls_with_key() {
        crate::test_util::trace_init();
        kafka_happy_path(
            "localhost:9092",
            None,
            Some(KafkaTlsConfig {
                enabled: Some(true),
                options: TlsOptions::test_options(),
            }),
            KafkaCompression::None,
        )
        .await;
    }

    #[tokio::test]
    async fn kafka_happy_path_sasl() {
        crate::test_util::trace_init();
        kafka_happy_path(
            "localhost:9093",
            Some(KafkaSaslConfig {
                enabled: Some(true),
                username: Some("admin".to_owned()),
                password: Some("admin".to_owned()),
                mechanism: Some("PLAIN".to_owned()),
            }),
            None,
            KafkaCompression::None,
        )
        .await;
    }

    async fn kafka_happy_path(
        server: &str,
        sasl: Option<KafkaSaslConfig>,
        tls: Option<KafkaTlsConfig>,
        compression: KafkaCompression,
    ) {
        let topic = format!("test-{}", random_string(10));
        let headers_key = "headers_key".to_string();
        let kafka_auth = KafkaAuthConfig { sasl, tls };
        let config = KafkaSinkConfig {
            bootstrap_servers: server.to_string(),
            topic: format!("{}-%Y%m%d", topic),
            key_field: None,
            encoding: EncodingConfig::from(StandardEncodings::Text),
            batch: BatchConfig::default(),
            compression,
            auth: kafka_auth.clone(),
            socket_timeout_ms: 60000,
            message_timeout_ms: 300000,
            librdkafka_options: HashMap::new(),
            headers_key: Some(headers_key.clone()),
        };
        let topic = format!("{}-{}", topic, chrono::Utc::now().format("%Y%m%d"));
        println!("Topic name generated in test: {:?}", topic);
        let (acker, ack_counter) = Acker::basic();
        let sink = KafkaSink::new(config, acker).unwrap();
        let sink = VectorSink::from_event_streamsink(sink);

        let num_events = 1000;
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input, events) = random_lines_with_stream(100, num_events, Some(batch));

        let header_1_key = "header-1-key";
        let header_1_value = "header-1-value";
        let input_events = events.map(move |mut events| {
            let headers_key = headers_key.clone();
            let mut header_values = BTreeMap::new();
            header_values.insert(
                header_1_key.to_string(),
                Value::Bytes(Bytes::from(header_1_value)),
            );
            events.for_each_log(move |log| {
                log.insert(headers_key.clone(), header_values.clone());
            });
            events
        });
        components::init_test();
        sink.run(input_events).await.unwrap();
        components::SINK_TESTS.assert(&["protocol"]);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        // read back everything from the beginning
        let mut client_config = rdkafka::ClientConfig::new();
        client_config.set("bootstrap.servers", server);
        client_config.set("group.id", &random_string(10));
        client_config.set("enable.partition.eof", "true");
        let _ = kafka_auth.apply(&mut client_config).unwrap();

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
                    let (header_key, header_val) = msg.headers().unwrap().get(0).unwrap();
                    assert_eq!(header_key, header_1_key);
                    assert_eq!(header_val, header_1_value.as_bytes());
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

        assert_eq!(
            ack_counter.load(std::sync::atomic::Ordering::Relaxed),
            num_events
        );
    }
}
