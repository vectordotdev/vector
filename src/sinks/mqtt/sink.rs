use std::time::Duration;

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use rumqttc::{AsyncClient, EventLoop};
use tokio_util::sync::CancellationToken;

use super::{
    MqttSinkConfig,
    config::MqttQoS,
    request_builder::{MqttEncoder, MqttRequestBuilder},
    service::MqttService,
};
use crate::{common::mqtt::MqttConnector, internal_events::MqttConnectionError, sinks::prelude::*};

/// Time that the event loop can use to send queued requests and finish QoS handshakes on shutdown.
const STOPPED_SINK_FLUSH_TIMEOUT: Duration = Duration::from_secs(5);

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
        let sink_stopped = CancellationToken::new();
        // Cancels `sink_stopped` when this future completes or is dropped.
        let _sink_stopped_guard = sink_stopped.clone().drop_guard();

        // This is necessary to keep the mqtt event loop moving forward.
        crate::spawn_in_current_span(drive_event_loop(
            connection,
            client.clone(),
            sink_stopped,
            STOPPED_SINK_FLUSH_TIMEOUT,
        ));

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

        input
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
            .run()
            .await
    }
}

/// Polls the MQTT event loop until the sink stops.
///
/// After `sink_stopped` is cancelled, the loop continues for at most `flush_timeout` to send
/// queued requests and finish QoS handshakes, or until the next connection error. Retaining
/// the client prevents `RequestsDone` from closing the socket before acknowledgements arrive.
async fn drive_event_loop(
    mut connection: EventLoop,
    _client: AsyncClient,
    sink_stopped: CancellationToken,
    flush_timeout: Duration,
) {
    let poll_loop = async {
        loop {
            // If an error is returned here there is currently no way to tie this back
            // to the event that was posted which means we can't accurately provide
            // delivery guarantees.
            // We need this issue resolved first:
            // https://github.com/bytebeamio/rumqtt/issues/349
            if let Err(error) = connection.poll().await {
                emit!(MqttConnectionError { error });
                if sink_stopped.is_cancelled() {
                    break;
                }
            }
        }
    };

    // `poll` can stay blocked while connected, for example when the broker does not acknowledge
    // publishes and the inflight window is full.
    let flush_deadline = async {
        sink_stopped.cancelled().await;
        tokio::time::sleep(flush_timeout).await;
    };

    tokio::select! {
        () = poll_loop => {}
        () = flush_deadline => {}
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

    use rstest::rstest;
    use rumqttc::{AsyncClient, MqttOptions, QoS};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::mpsc::{UnboundedReceiver, unbounded_channel},
        task::JoinHandle,
    };
    use tokio_util::sync::CancellationToken;

    use super::drive_event_loop;

    const TEST_TIMEOUT: Duration = Duration::from_secs(10);

    /// Starts a mock broker that reports each accepted connection. If `accept` is true, it sends
    /// CONNACK and keeps the connection open. If not, it closes the connection before CONNACK.
    async fn spawn_broker(accept: bool) -> (u16, UnboundedReceiver<()>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (connections_tx, connections_rx) = unbounded_channel();

        let broker = tokio::spawn(async move {
            let mut open_streams = Vec::new();
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                connections_tx.send(()).unwrap();
                if accept {
                    // CONNACK: session not present, connection accepted.
                    stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
                    open_streams.push(stream);
                }
            }
        });

        (port, connections_rx, broker)
    }

    fn client(port: u16) -> (AsyncClient, rumqttc::EventLoop) {
        AsyncClient::new(MqttOptions::new("vector-test", "127.0.0.1", port), 10)
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

    #[tokio::test]
    async fn event_loop_completes_qos2_handshake_after_sink_stops() {
        tokio::time::timeout(TEST_TIMEOUT, async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let (client, connection) = client(listener.local_addr().unwrap().port());
            client
                .publish("test", QoS::ExactlyOnce, false, "message")
                .await
                .unwrap();
            let sink_stopped = CancellationToken::new();
            sink_stopped.cancel();
            let event_loop = tokio::spawn(drive_event_loop(
                connection,
                client,
                sink_stopped,
                Duration::from_secs(1),
            ));

            let (mut stream, _) = listener.accept().await.unwrap();
            assert_eq!(read_packet(&mut stream).await[0], 0x10); // CONNECT
            stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap(); // CONNACK
            let publish = read_packet(&mut stream).await;
            assert_eq!(publish[0], 0x34); // PUBLISH, QoS 2
            assert_eq!(&publish[2..8], b"\x00\x04test");
            assert_eq!(&publish[10..], b"message");
            let pkid = &publish[8..10];

            // Let the client poll with an empty request queue before acknowledging the publish.
            tokio::time::timeout(Duration::from_millis(100), stream.read_u8())
                .await
                .expect_err("connection closed before PUBREC");
            stream
                .write_all(&[0x50, 0x02, pkid[0], pkid[1]])
                .await
                .unwrap(); // PUBREC
            assert_eq!(
                read_packet(&mut stream).await,
                [0x62, 0x02, pkid[0], pkid[1]] // PUBREL
            );
            tokio::time::timeout(Duration::from_millis(100), stream.read_u8())
                .await
                .expect_err("connection closed before PUBCOMP");
            stream
                .write_all(&[0x70, 0x02, pkid[0], pkid[1]])
                .await
                .unwrap(); // PUBCOMP

            event_loop.await.unwrap();
            assert_eq!(stream.read(&mut [0]).await.unwrap(), 0);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        })
        .await
        .expect("QoS 2 shutdown did not finish");
    }

    #[tokio::test]
    async fn event_loop_stops_retrying_after_sink_stops() {
        let (port, mut connections, broker) = spawn_broker(false).await;
        let (client, connection) = client(port);
        let sink_stopped = CancellationToken::new();
        sink_stopped.cancel();

        // The flush timeout is long, so only the connection error can end the loop.
        tokio::time::timeout(
            TEST_TIMEOUT,
            drive_event_loop(connection, client, sink_stopped, Duration::from_hours(1)),
        )
        .await
        .expect("event loop kept retrying after the sink stopped");
        broker.abort();

        assert_eq!(connections.recv().await, Some(()));
        assert_eq!(connections.try_recv().ok(), None);
    }

    #[rstest]
    #[case::idle(0)]
    #[case::full_inflight_window(2)]
    #[tokio::test]
    async fn event_loop_stops_after_flush_timeout_when_poll_is_blocked(#[case] publishes: usize) {
        let (port, mut connections, broker) = spawn_broker(true).await;
        let (client, mut connection) = client(port);
        connection.mqtt_options.set_inflight(1);
        for _ in 0..publishes {
            client
                .publish("test", QoS::ExactlyOnce, false, "message")
                .await
                .unwrap();
        }
        let sink_stopped = CancellationToken::new();

        let event_loop = tokio::spawn(drive_event_loop(
            connection,
            client,
            sink_stopped.clone(),
            Duration::from_millis(100),
        ));
        let connection = tokio::time::timeout(TEST_TIMEOUT, connections.recv())
            .await
            .expect("event loop did not connect to the broker");
        assert_eq!(connection, Some(()));
        sink_stopped.cancel();

        tokio::time::timeout(TEST_TIMEOUT, event_loop)
            .await
            .expect("event loop kept polling after the flush timeout")
            .unwrap();
        broker.abort();
        assert_eq!(connections.try_recv().ok(), None);
    }
}
