use std::time::Duration;

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

/// Time that the event loop can use to send requests that are queued when the sink stops.
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
/// queued requests. It ends earlier on `RequestsDone`, which `poll` reports while connected after
/// the client is dropped and the queued requests are sent, or on the next connection error.
async fn drive_event_loop(
    mut connection: EventLoop,
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
            match connection.poll().await {
                Ok(_) => {}
                // The sink dropped its client. Polling again would reconnect to the broker.
                Err(ConnectionError::RequestsDone) => break,
                Err(error) => {
                    emit!(MqttConnectionError { error });
                    if sink_stopped.is_cancelled() {
                        break;
                    }
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

    use rumqttc::{AsyncClient, MqttOptions};
    use tokio::{
        io::AsyncWriteExt,
        net::TcpListener,
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

    #[tokio::test]
    async fn event_loop_stops_without_reconnecting_after_client_is_dropped() {
        let (port, mut connections, broker) = spawn_broker(true).await;
        let (client, connection) = client(port);
        drop(client);

        // The sink is still running, so only `RequestsDone` can end the loop.
        tokio::time::timeout(
            TEST_TIMEOUT,
            drive_event_loop(connection, CancellationToken::new(), Duration::ZERO),
        )
        .await
        .expect("event loop kept polling after the client was dropped");
        broker.abort();

        assert_eq!(connections.recv().await, Some(()));
        assert_eq!(connections.try_recv().ok(), None);
    }

    #[tokio::test]
    async fn event_loop_stops_retrying_after_sink_stops() {
        let (port, mut connections, broker) = spawn_broker(false).await;
        let (client, connection) = client(port);
        drop(client);
        let sink_stopped = CancellationToken::new();
        sink_stopped.cancel();

        // The flush timeout is long, so only the connection error can end the loop.
        tokio::time::timeout(
            TEST_TIMEOUT,
            drive_event_loop(connection, sink_stopped, Duration::from_hours(1)),
        )
        .await
        .expect("event loop kept retrying after the sink stopped");
        broker.abort();

        assert_eq!(connections.recv().await, Some(()));
        assert_eq!(connections.try_recv().ok(), None);
    }

    #[tokio::test]
    async fn event_loop_stops_after_flush_timeout_when_poll_is_blocked() {
        let (port, mut connections, broker) = spawn_broker(true).await;
        // The client stays alive, so `poll` waits for requests and never reports `RequestsDone`.
        let (_client, connection) = client(port);
        let sink_stopped = CancellationToken::new();

        let event_loop = tokio::spawn(drive_event_loop(
            connection,
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
    }
}
