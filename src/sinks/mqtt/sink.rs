use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use rumqttc::{ConnectionError, EventLoop};
use tokio_util::sync::CancellationToken;

use super::{
    MqttSinkConfig,
    config::MqttQoS,
    request_builder::{MqttEncoder, MqttRequestBuilder},
    service::MqttService,
};
use crate::{common::mqtt::MqttConnector, internal_events::MqttConnectionError, sinks::prelude::*};

pub struct MqttSink {
    transformer: Transformer,
    encoder: Encoder<()>,
    connector: MqttConnector,
    topic: ConfinedTemplate,
    quality_of_service: MqttQoS,
    retain: bool,
}

pub(super) struct MqttEvent {
    pub(super) topic: String,
    pub(super) event: Event,
}

impl MqttSink {
    pub fn new(
        config: &MqttSinkConfig,
        topic: ConfinedTemplate,
        connector: MqttConnector,
    ) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);

        Ok(Self {
            transformer,
            encoder,
            connector,
            topic,
            quality_of_service: config.quality_of_service,
            retain: config.retain,
        })
    }

    fn make_mqtt_event(&self, event: Event) -> Option<MqttEvent> {
        let topic = self
            .topic
            .render_string(&event)
            .map_err(|missing_keys| {
                emit!(TemplateRenderingError {
                    error: missing_keys,
                    field: Some("topic"),
                    drop_event: true,
                });
            })
            .ok()?;

        Some(MqttEvent { topic, event })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let (client, connection) = self.connector.connect();
        let input_done = CancellationToken::new();

        let service = ServiceBuilder::new().service(MqttService {
            client,
            quality_of_service: self.quality_of_service,
            retain: self.retain,
        });

        let request_builder = MqttRequestBuilder {
            encoder: MqttEncoder {
                encoder: self.encoder.clone(),
                transformer: self.transformer.clone(),
            },
        };

        let driver = input
            .filter_map(|event| std::future::ready(self.make_mqtt_event(event)))
            .request_builder(default_request_builder_concurrency_limit(), request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build MQTT request: {e:?}.");
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(service)
            .protocol("mqtt")
            .run();

        // Keep the event loop within the sink's lifetime so topology shutdown waits for the
        // drain, and cancelling the sink also closes its connection.
        let (result, drained) = tokio::join!(
            async {
                let result = driver.await;
                input_done.cancel();
                result
            },
            drive_event_loop(connection, input_done.clone()),
        );
        result.and(drained)
    }
}

/// Dropping the driver's clients lets rumqttc drain queued requests and QoS exchanges.
/// In 0.34, `RequestsDone` is returned only after that outbound work has completed.
async fn drive_event_loop(
    mut connection: EventLoop,
    input_done: CancellationToken,
) -> Result<(), ()> {
    if input_done.is_cancelled() {
        let state = connection.diagnostics();
        if state.queues.requests_rx_len == 0
            && state.queues.pending_len == 0
            && state.outbound.outbound_drained
        {
            // Empty input needs no connection, including when the broker is unavailable.
            return Ok(());
        }
    }

    loop {
        match connection.poll().await {
            Ok(_) => {}
            Err(ConnectionError::RequestsDone) => return Ok(()),
            Err(error) => {
                emit!(MqttConnectionError { error });
                if input_done.is_cancelled() {
                    return Ok(());
                }
            }
        }
    }
}

#[async_trait]
impl StreamSink<Event> for MqttSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;
    use rstest::rstest;
    use rumqttc::{AsyncClient, EventLoop, MqttOptions, PublishOptions, QoS};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    use tokio_util::sync::CancellationToken;
    use vector_lib::extra_context::ExtraContext;

    use super::drive_event_loop;
    use crate::{
        common::mqtt::MqttCommonConfig,
        config::{ComponentKey, Config, SinkConfig, SinkContext},
        event::{Event, LogEvent},
        sinks::{
            VectorSink,
            mqtt::{MqttSinkConfig, config::MqttQoS},
        },
        template::Template,
        test_util::{mock::basic_source, start_topology, trace_init},
    };

    const TEST_TIMEOUT: Duration = Duration::from_secs(10);

    async fn sink(port: u16, quality_of_service: MqttQoS) -> VectorSink {
        MqttSinkConfig {
            common: MqttCommonConfig {
                host: "127.0.0.1".into(),
                port,
                ..Default::default()
            },
            topic: Template::try_from("test").unwrap(),
            quality_of_service,
            ..Default::default()
        }
        .build(SinkContext::default())
        .await
        .unwrap()
        .0
    }

    fn client(port: u16, inflight: u16) -> (AsyncClient, EventLoop) {
        let mut options = MqttOptions::new("vector-test", ("127.0.0.1", port));
        options.set_clean_session(false).set_inflight(inflight);
        AsyncClient::builder(options).capacity(10).build()
    }

    async fn publish(client: &AsyncClient, qos: QoS, payload: &'static [u8]) {
        client
            .publish(
                "test",
                Bytes::from_static(payload),
                PublishOptions::new(qos),
            )
            .await
            .unwrap();
    }

    /// Reads the short MQTT packets used by these tests.
    async fn read_packet(stream: &mut TcpStream) -> Vec<u8> {
        let header = stream.read_u8().await.unwrap();
        let len = stream.read_u8().await.unwrap();
        assert!(len < 128, "test packet needs a multi-byte remaining length");
        let mut packet = vec![0; usize::from(len) + 2];
        packet[0] = header;
        packet[1] = len;
        stream.read_exact(&mut packet[2..]).await.unwrap();
        packet
    }

    async fn accept_connection(listener: &TcpListener) -> TcpStream {
        let (mut stream, _) = listener.accept().await.unwrap();
        let connect = read_packet(&mut stream).await;
        assert_eq!(connect[0], 0x10);
        assert_eq!(&connect[2..9], b"\x00\x04MQTT\x04", "expected MQTT 3.1.1");
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
        stream
    }

    /// Acknowledges publishes and releases, returning true for a completed QoS exchange.
    async fn acknowledge(stream: &mut TcpStream, packet: &[u8]) -> bool {
        let (header, pkid, complete) = match packet[0] & !0x08 {
            0x32 => (0x40, &packet[8..10], true),
            0x34 => (0x50, &packet[8..10], false),
            0x62 => (0x70, &packet[2..4], true),
            _ => panic!("unexpected MQTT packet: {packet:?}"),
        };
        stream
            .write_all(&[header, 2, pkid[0], pkid[1]])
            .await
            .unwrap();
        complete
    }

    #[rstest]
    #[case::qos0(MqttQoS::AtMostOnce, 0x30)]
    #[case::qos1(MqttQoS::AtLeastOnce, 0x32)]
    #[case::qos2(MqttQoS::ExactlyOnce, 0x34)]
    #[tokio::test]
    async fn sink_drains_before_returning(#[case] qos: MqttQoS, #[case] header: u8) {
        tokio::time::timeout(TEST_TIMEOUT, async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let sink = sink(listener.local_addr().unwrap().port(), qos).await;
            let task = tokio::spawn(sink.run_events([Event::Log(LogEvent::from("message"))]));
            let mut stream = accept_connection(&listener).await;
            let packet = read_packet(&mut stream).await;
            assert_eq!(packet[0], header);
            assert_eq!(&packet[2..8], b"\x00\x04test");
            let payload_offset = if header == 0x30 { 8 } else { 10 };
            let payload: serde_json::Value =
                serde_json::from_slice(&packet[payload_offset..]).unwrap();
            assert_eq!(payload["message"], "message");

            if header != 0x30 {
                tokio::time::timeout(Duration::from_millis(100), stream.read_u8())
                    .await
                    .expect_err("connection closed before acknowledgement");
                assert!(!task.is_finished(), "sink returned before acknowledgement");
                // A slow broker must not be cut off by a sink-local five-second timer.
                tokio::time::pause();
                tokio::time::advance(Duration::from_secs(6)).await;
                assert!(!task.is_finished(), "sink imposed its own drain deadline");
                tokio::time::resume();
                if !acknowledge(&mut stream, &packet).await {
                    let release = read_packet(&mut stream).await;
                    assert_eq!(release, [0x62, 2, packet[8], packet[9]]);
                    tokio::time::timeout(Duration::from_millis(100), stream.read_u8())
                        .await
                        .expect_err("connection closed before PUBCOMP");
                    assert!(!task.is_finished(), "sink returned before PUBCOMP");
                    assert!(acknowledge(&mut stream, &release).await);
                }
            }
            tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .expect("drained sink did not finish promptly")
                .unwrap()
                .unwrap();
            assert_eq!(stream.read(&mut [0]).await.unwrap(), 0);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn idle_sink_closes_when_input_ends() {
        tokio::time::timeout(TEST_TIMEOUT, async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let sink = sink(listener.local_addr().unwrap().port(), MqttQoS::ExactlyOnce).await;
            let (input, events) = futures::channel::mpsc::unbounded();
            let task = tokio::spawn(sink.run(events));
            let mut stream = accept_connection(&listener).await;
            drop(input);
            tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .expect("idle sink did not finish promptly")
                .unwrap()
                .unwrap();
            // Closing before CONNACK is read can reset the socket instead of sending FIN.
            let result = stream.read(&mut [0]).await;
            assert!(
                matches!(result, Ok(0))
                    || matches!(&result, Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset),
                "idle connection did not close: {result:?}"
            );
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn cancelling_sink_closes_unacknowledged_connection() {
        tokio::time::timeout(TEST_TIMEOUT, async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let sink = sink(listener.local_addr().unwrap().port(), MqttQoS::ExactlyOnce).await;
            let task = tokio::spawn(sink.run_events([Event::Log(LogEvent::from("message"))]));
            let mut stream = accept_connection(&listener).await;
            assert_eq!(read_packet(&mut stream).await[0], 0x34);
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(1), stream.read(&mut [0]))
                    .await
                    .expect("cancelled sink left its connection open")
                    .unwrap(),
                0
            );
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn drain_connection_error_finishes_sink_without_reconnecting() {
        tokio::time::timeout(TEST_TIMEOUT, async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let sink = sink(listener.local_addr().unwrap().port(), MqttQoS::ExactlyOnce).await;
            let task = tokio::spawn(sink.run_events([Event::Log(LogEvent::from("message"))]));
            let mut stream = accept_connection(&listener).await;
            assert_eq!(read_packet(&mut stream).await[0], 0x34);
            drop(stream);
            task.await.unwrap().unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        })
        .await
        .unwrap();
    }

    #[rstest]
    #[case::acknowledged(false)]
    #[case::broker_disconnect(true)]
    #[tokio::test]
    async fn reload_waits_for_qos_drain(#[case] disconnect: bool) {
        trace_init();
        tokio::time::timeout(
            TEST_TIMEOUT,
            Box::pin(async {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let mqtt = MqttSinkConfig {
                    common: MqttCommonConfig {
                        host: "127.0.0.1".into(),
                        port: listener.local_addr().unwrap().port(),
                        ..Default::default()
                    },
                    topic: Template::try_from("test").unwrap(),
                    quality_of_service: MqttQoS::ExactlyOnce,
                    ..Default::default()
                };
                let (mut input, source) = basic_source();
                let mut config = Config::builder();
                config.add_source("input", source);
                config.add_sink("mqtt", &["input"], mqtt.clone());
                let mut replacement = config.clone();
                replacement.sinks[&ComponentKey::from("mqtt")].inner = Box::new(MqttSinkConfig {
                    retain: true,
                    ..mqtt
                });
                let (mut topology, _crash) = start_topology(config.build().unwrap(), false).await;
                input
                    .send_event(Event::Log(LogEvent::from("before reload")))
                    .await
                    .unwrap();
                let mut stream = accept_connection(&listener).await;
                let publish = read_packet(&mut stream).await;
                assert_eq!(publish[0], 0x34);

                {
                    let mut reload = Box::pin(topology.reload_config_and_respawn(
                        replacement.build().unwrap(),
                        ExtraContext::default(),
                    ));
                    tokio::time::timeout(Duration::from_millis(150), &mut reload)
                        .await
                        .expect_err("reload abandoned the QoS exchange");
                    if disconnect {
                        drop(stream);
                    } else {
                        assert!(!acknowledge(&mut stream, &publish).await);
                        let release = read_packet(&mut stream).await;
                        assert!(acknowledge(&mut stream, &release).await);
                    }
                    reload.await.unwrap();
                }
                let mut replacement = accept_connection(&listener).await;
                input
                    .send_event(Event::Log(LogEvent::from("after reload")))
                    .await
                    .unwrap();
                let publish = read_packet(&mut replacement).await;
                assert_eq!(
                    publish[0], 0x35,
                    "replacement retain setting was not applied"
                );
                let payload: serde_json::Value = serde_json::from_slice(&publish[10..]).unwrap();
                assert_eq!(payload["message"], "after reload");
                replacement
                    .write_all(&[0x50, 2, publish[8], publish[9]])
                    .await
                    .unwrap();
                let release = read_packet(&mut replacement).await;
                assert!(acknowledge(&mut replacement, &release).await);
                assert!(topology.stop().await);
            }),
        )
        .await
        .unwrap();
    }

    #[rstest]
    #[case::qos1(QoS::AtLeastOnce)]
    #[case::qos2(QoS::ExactlyOnce)]
    #[tokio::test]
    async fn drain_handles_queued_publishes_and_reused_ids(#[case] qos: QoS) {
        tokio::time::timeout(TEST_TIMEOUT, async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let (client, connection) = client(listener.local_addr().unwrap().port(), 2);
            for _ in 0..4 {
                publish(&client, qos, b"message").await;
            }
            let done = CancellationToken::new();
            done.cancel();
            drop(client);
            let task = tokio::spawn(drive_event_loop(connection, done));
            let mut stream = accept_connection(&listener).await;
            let mut completed = 0;
            while completed < 4 {
                let packet = read_packet(&mut stream).await;
                completed += usize::from(acknowledge(&mut stream, &packet).await);
            }
            task.await.unwrap().unwrap();
            assert_eq!(stream.read(&mut [0]).await.unwrap(), 0);
        })
        .await
        .unwrap();
    }

    #[rstest]
    #[case::qos1_resume(QoS::AtLeastOnce, false, true, false)]
    #[case::qos1_reset(QoS::AtLeastOnce, false, false, false)]
    #[case::qos1_duplicate_ack_reset(QoS::AtLeastOnce, false, false, true)]
    #[case::qos2_resume_publish(QoS::ExactlyOnce, false, true, false)]
    #[case::qos2_resume_release(QoS::ExactlyOnce, true, true, false)]
    #[case::qos2_reset_release(QoS::ExactlyOnce, true, false, false)]
    #[tokio::test]
    async fn drain_after_reconnect(
        #[case] qos: QoS,
        #[case] released: bool,
        #[case] session_present: bool,
        #[case] duplicate_ack: bool,
    ) {
        tokio::time::timeout(TEST_TIMEOUT, async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let (client, connection) = client(listener.local_addr().unwrap().port(), 1);
            publish(&client, qos, b"message").await;
            publish(&client, qos, b"message").await;
            let done = CancellationToken::new();
            let task = tokio::spawn(drive_event_loop(connection, done.clone()));
            let mut stream = accept_connection(&listener).await;
            let packet = read_packet(&mut stream).await;
            if released {
                assert!(!acknowledge(&mut stream, &packet).await);
                assert_eq!(
                    read_packet(&mut stream).await,
                    [0x62, 2, packet[8], packet[9]]
                );
            }
            if duplicate_ack {
                // rumqttc processes the first ACK, then fails on the duplicate in the same read.
                let ack = [0x40, 2, packet[8], packet[9]];
                stream.write_all(&[ack, ack].concat()).await.unwrap();
            } else {
                drop(stream);
            }
            let (mut stream, _) = listener.accept().await.unwrap();
            assert_eq!(read_packet(&mut stream).await[0], 0x10);
            // This request is in the client channel, not in the previous session's replay queue.
            publish(&client, qos, b"later").await;
            stream
                .write_all(&[0x20, 2, u8::from(session_present), 0])
                .await
                .unwrap();
            drop(client);
            done.cancel();
            let mut saw_new_publish = false;
            while stream.peek(&mut [0]).await.unwrap() != 0 {
                let packet = read_packet(&mut stream).await;
                if packet[0] >> 4 == 3 {
                    saw_new_publish |= &packet[10..] == b"later";
                }
                acknowledge(&mut stream, &packet).await;
            }
            assert!(
                saw_new_publish,
                "new work was discarded with the old session"
            );
            task.await.unwrap().unwrap();
            assert_eq!(stream.read(&mut [0]).await.unwrap(), 0);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn rejected_publish_does_not_block_draining() {
        let (client, connection) = client(0, 1);
        assert!(
            client
                .publish("invalid/#", Bytes::new(), PublishOptions::at_least_once())
                .await
                .is_err()
        );
        drop(client);
        let done = CancellationToken::new();
        done.cancel();
        // There is no broker: a completed rejection must not leave any work to drain.
        tokio::time::timeout(Duration::from_secs(1), drive_event_loop(connection, done))
            .await
            .unwrap()
            .unwrap();
    }
}
