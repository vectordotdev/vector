use std::sync::Arc;

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use rumqttc::{Event as MqttNotification, EventLoop, Incoming, Outgoing};
use tokio_util::sync::CancellationToken;

use super::{
    MqttSinkConfig,
    config::MqttQoS,
    request_builder::{MqttEncoder, MqttRequestBuilder},
    service::{MqttClient, MqttService},
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
        let client = Arc::new(MqttClient::new(client));
        let input_done = CancellationToken::new();

        let service = ServiceBuilder::new().service(MqttService {
            client: Arc::clone(&client),
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
            drive_event_loop(connection, client, input_done.clone()),
        );
        result.and(drained)
    }
}

#[derive(Clone, Copy, Default)]
struct PublishState {
    awaiting_ack: bool,
    awaiting_comp: bool,
}

/// Associates terminal acknowledgements with publishes, including replay and packet-ID reuse.
struct PublishTracker {
    states: Vec<PublishState>,
}

impl PublishTracker {
    fn new(max_inflight: u16) -> Self {
        Self {
            states: vec![PublishState::default(); usize::from(max_inflight) + 1],
        }
    }

    fn completed(&mut self, event: MqttNotification) -> usize {
        let awaiting = match event {
            MqttNotification::Outgoing(Outgoing::Publish(0)) => return 1,
            MqttNotification::Outgoing(Outgoing::Publish(pkid)) => {
                self.states[usize::from(pkid)].awaiting_ack = true;
                return 0;
            }
            MqttNotification::Outgoing(Outgoing::PubRel(pkid)) => {
                let state = &mut self.states[usize::from(pkid)];
                state.awaiting_ack = false;
                state.awaiting_comp = true;
                return 0;
            }
            MqttNotification::Incoming(Incoming::PubAck(ack)) => self
                .states
                .get_mut(usize::from(ack.pkid))
                .map(|state| &mut state.awaiting_ack),
            MqttNotification::Incoming(Incoming::PubComp(ack)) => self
                .states
                .get_mut(usize::from(ack.pkid))
                .map(|state| &mut state.awaiting_comp),
            _ => return 0,
        };
        // rumqttc can leave an unsolicited acknowledgement in its event queue after an error.
        // Only retire a publish in the corresponding phase. Replay can reuse an ID while an
        // earlier PUBREL is still outstanding, so the two phases are tracked independently.
        awaiting.map_or(0, |awaiting| usize::from(std::mem::take(awaiting)))
    }

    fn reset(&mut self) {
        self.states.fill(PublishState::default());
    }
}

/// Drains queued publishes and QoS exchanges after input ends, without a sink-local deadline.
/// Retaining the client prevents `RequestsDone` from closing the socket before acknowledgements.
async fn drive_event_loop(
    mut connection: EventLoop,
    client: Arc<MqttClient>,
    input_done: CancellationToken,
) -> Result<(), ()> {
    let mut tracker = PublishTracker::new(connection.mqtt_options.inflight());
    let mut draining = false;
    loop {
        draining |= input_done.is_cancelled();
        if draining && !client.has_pending() {
            return Ok(());
        }

        // On a fresh broker session rumqttc discards its replay queue and any collided publish.
        // Those publishes must also leave our count. Requests still in the client channel remain.
        let replayed = if connection.network.is_none() {
            connection.pending.len() + usize::from(connection.state.collision.is_some())
        } else {
            0
        };
        let result = {
            let poll = connection.poll();
            tokio::pin!(poll);
            tokio::select! {
                result = &mut poll => result,
                () = input_done.cancelled(), if !draining => {
                    draining = true;
                    if !client.has_pending() {
                        return Ok(());
                    }
                    // Do not cancel an in-progress write just because input ended.
                    poll.await
                }
            }
        };

        match result {
            Ok(MqttNotification::Incoming(Incoming::ConnAck(ack))) if !ack.session_present => {
                client.complete(replayed);
            }
            Ok(event) => client.complete(tracker.completed(event)),
            Err(error) => {
                emit!(MqttConnectionError { error });
                if input_done.is_cancelled() {
                    return Err(());
                }
                // A network read can process several acknowledgements before a later read fails.
                // Consume those buffered events before replay so each publish is counted once.
                while let Some(event) = connection.state.events.pop_front() {
                    client.complete(tracker.completed(event));
                }
                // Replay will register packet IDs again, without counting new submissions.
                tracker.reset();
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
    use std::{sync::Arc, time::Duration};

    use bytes::Bytes;
    use rstest::rstest;
    use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    use tokio_util::sync::CancellationToken;

    use super::drive_event_loop;
    use crate::{
        common::mqtt::MqttCommonConfig,
        config::{SinkConfig, SinkContext},
        event::{Event, LogEvent},
        sinks::{
            VectorSink,
            mqtt::{MqttSinkConfig, config::MqttQoS, service::MqttClient},
        },
        template::Template,
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

    fn client(port: u16, inflight: u16) -> (Arc<MqttClient>, EventLoop) {
        let mut options = MqttOptions::new("vector-test", "127.0.0.1", port);
        options.set_clean_session(false).set_inflight(inflight);
        let (client, connection) = AsyncClient::new(options, 10);
        (Arc::new(MqttClient::new(client)), connection)
    }

    async fn publish(client: &MqttClient, qos: QoS, payload: &'static [u8]) {
        client
            .publish("test".into(), qos, false, Bytes::from_static(payload))
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
        assert_eq!(read_packet(&mut stream).await[0], 0x10);
        stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
        stream
    }

    /// Acknowledges publishes and releases, returning true for a completed QoS exchange.
    async fn acknowledge(stream: &mut TcpStream, packet: &[u8]) -> bool {
        let (header, pkid, complete) = match packet[0] {
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
    async fn drain_connection_error_fails_without_reconnecting() {
        tokio::time::timeout(TEST_TIMEOUT, async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let (client, connection) = client(listener.local_addr().unwrap().port(), 1);
            publish(&client, QoS::ExactlyOnce, b"message").await;
            let done = CancellationToken::new();
            done.cancel();
            let task = tokio::spawn(drive_event_loop(connection, client, done));
            let mut stream = accept_connection(&listener).await;
            assert_eq!(read_packet(&mut stream).await[0], 0x34);
            drop(stream);
            assert!(task.await.unwrap().is_err());
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
            let task = tokio::spawn(drive_event_loop(connection, client, done));
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
            let task = tokio::spawn(drive_event_loop(
                connection,
                Arc::clone(&client),
                done.clone(),
            ));
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
            done.cancel();
            let expected = if session_present { 3 } else { 1 };
            let mut completed = 0;
            let mut saw_new_publish = false;
            while completed < expected {
                let packet = read_packet(&mut stream).await;
                if packet[0] >> 4 == 3 {
                    saw_new_publish |= &packet[10..] == b"later";
                }
                completed += usize::from(acknowledge(&mut stream, &packet).await);
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
                .publish("invalid/#".into(), QoS::AtLeastOnce, false, Bytes::new())
                .await
                .is_err()
        );
        let done = CancellationToken::new();
        done.cancel();
        // There is no broker: a completed rejection must not leave any work to drain.
        tokio::time::timeout(
            Duration::from_secs(1),
            drive_event_loop(connection, client, done),
        )
        .await
        .unwrap()
        .unwrap();
    }
}
