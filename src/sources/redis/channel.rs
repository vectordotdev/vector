use std::time::Duration;

use futures_util::StreamExt;
use snafu::Snafu;
use tracing::{trace, warn};

use crate::{
    common::backoff::ExponentialBackoff,
    internal_events::{RedisConnectionError, RedisConnectionEstablished, RedisReceiveEventError},
    sources::{
        Source,
        redis::{ConnectionInfo, InputHandler},
    },
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to create connection: {}", source))]
    Connection { source: redis::RedisError },
    #[snafu(display("Failed to subscribe to channel: {}", source))]
    Subscribe { source: redis::RedisError },
}

impl BuildError {
    /// The underlying Redis error, regardless of which stage (connect or subscribe) failed.
    fn into_source(self) -> redis::RedisError {
        match self {
            BuildError::Connection { source } | BuildError::Subscribe { source } => source,
        }
    }
}

/// Defines how a pub/sub "session" ended.
///
/// A session = we connected to Redis, SUBSCRIBE'd to a channel,
/// and started reading messages in a loop.
enum SessionEnd {
    /// Vector is shutting down; stop and don't reconnect.
    Shutdown,
    /// Redis connection dropped; we should reconnect.
    Disconnected,
    /// Downstream stopped accepting events; there's no point continuing.
    DownstreamClosed,
}

impl InputHandler {
    /// Build the Redis `channel` source.
    ///
    /// The source runs a reconnect loop: it connects, SUBSCRIBEs, and streams messages
    /// until either shutdown fires, downstream closes, or the Redis connection drops. On a
    /// drop it reconnects with exponential backoff instead of stopping, so a Redis restart
    /// or transient network blip no longer requires a manual Vector restart.
    pub(super) async fn subscribe(
        mut self,
        connection_info: ConnectionInfo,
    ) -> crate::Result<Source> {
        let client = self.client.clone();
        let channel = self.key.clone();
        let endpoint = connection_info.endpoint.to_string();

        /// Open a pubsub connection and SUBSCRIBE to `channel`.
        /// Returns a ready `PubSub` on success.
        async fn connect_and_subscribe(
            client: &redis::Client,
            endpoint: &str,
            channel: &str,
        ) -> Result<redis::aio::PubSub, BuildError> {
            // create pubsub connection
            let mut pubsub_conn = client
                .get_async_pubsub()
                .await
                .map_err(|source| BuildError::Connection { source })?;

            trace!(endpoint, "Connected.");

            // subscribe to the configured channel
            pubsub_conn
                .subscribe(channel)
                .await
                .map_err(|source| BuildError::Subscribe { source })?;

            trace!(endpoint, channel, "Subscribed to channel.");

            Ok(pubsub_conn)
        }

        async fn run_subscription_session<S>(
            pubsub_conn: &mut redis::aio::PubSub,
            channel: &str,
            shutdown: &mut S,
            handler: &mut InputHandler,
            endpoint: &str,
        ) -> SessionEnd
        where
            S: std::future::Future + Unpin,
        {
            let mut stream = pubsub_conn.on_message();

            loop {
                // One "step" in the session: either we got a message,
                // Redis dropped us, or shutdown fired.
                enum RecvEvent {
                    Msg(redis::Msg),
                    Shutdown,
                    Disconnected,
                }

                let event = tokio::select! {
                    maybe_msg = stream.next() => {
                        match maybe_msg {
                            Some(msg) => RecvEvent::Msg(msg),
                            None => RecvEvent::Disconnected,
                        }
                    }
                    _ = &mut *shutdown => {
                        RecvEvent::Shutdown
                    }
                };

                match event {
                    RecvEvent::Msg(msg) => match msg.get_payload::<String>() {
                        Ok(line) => {
                            // If downstream is gone and won't take more data,
                            // stop the source too.
                            if let Err(()) = handler.handle_line(line).await {
                                return SessionEnd::DownstreamClosed;
                            }
                        }
                        Err(error) => {
                            // Bad payload. We just log and keep going.
                            emit!(RedisReceiveEventError::from(error));
                        }
                    },

                    RecvEvent::Disconnected => {
                        // Redis connection ended (e.g. server restart).
                        // We'll reconnect in the outer loop.
                        warn!(
                            endpoint,
                            channel, "Redis pubsub stream ended; will reconnect."
                        );
                        return SessionEnd::Disconnected;
                    }

                    RecvEvent::Shutdown => {
                        // Vector shutdown. Caller will not reconnect.
                        return SessionEnd::Shutdown;
                    }
                }
            }
        }

        Ok(Box::pin(async move {
            // `shutdown` is a signal that resolves when Vector is stopping.
            let mut shutdown = self.cx.shutdown.clone();

            // Exponential backoff between reconnect attempts: 500ms, 1s, 2s, 4s, ...
            // capped at 30s. Matches the strategy used by other reconnecting sources
            // (e.g. `aws_s3`/`sqs`). Reset once a connection is successfully established.
            let mut backoff = ExponentialBackoff::from_millis(2)
                .factor(250)
                .max_delay(Duration::from_secs(30));

            // Whether we're recovering from a dropped/failed connection. Drives the
            // log/metric emitted on a successful (re)connect.
            let mut reconnecting = false;

            loop {
                // Connect + SUBSCRIBE, raced with shutdown. A connect against a black-holed
                // endpoint can stall for a long time, so we must still observe shutdown
                // promptly instead of waiting for the force-shutdown deadline. `biased`
                // checks shutdown first so it always wins when both are ready.
                let connect_result = tokio::select! {
                    biased;
                    _ = &mut shutdown => break,
                    res = connect_and_subscribe(&client, &endpoint, &channel) => res,
                };

                let mut pubsub_conn = match connect_result {
                    Ok(conn) => {
                        emit!(RedisConnectionEstablished {
                            reconnect: reconnecting
                        });
                        conn
                    }
                    Err(err) => {
                        // failed to connect or SUBSCRIBE
                        emit!(RedisConnectionError::from(err.into_source()));
                        reconnecting = true;

                        // back off before retrying, unless we're shutting down
                        let delay = backoff.next().expect("backoff never ends");
                        tokio::select! {
                            _ = tokio::time::sleep(delay) => continue,
                            _ = &mut shutdown => break,
                        }
                    }
                };

                // Connected: reset backoff so the next outage starts from the shortest delay.
                backoff.reset();
                reconnecting = false;

                // run that session (receive messages, forward them, etc.)
                let end_reason = run_subscription_session(
                    &mut pubsub_conn,
                    &channel,
                    &mut shutdown,
                    &mut self,
                    &endpoint,
                )
                .await;

                // We deliberately do not `UNSUBSCRIBE` here: on shutdown or a dropped
                // connection, awaiting that network round trip could block graceful shutdown
                // if Redis is slow or the socket is half-open. Dropping `pubsub_conn` closes
                // the connection and Redis releases the subscription automatically.
                match end_reason {
                    SessionEnd::Shutdown | SessionEnd::DownstreamClosed => {
                        // shutting down cleanly, or downstream closed: stop for good.
                        break;
                    }

                    SessionEnd::Disconnected => {
                        // Redis dropped us. Reconnect after a backoff, unless shutdown fires
                        // during that backoff. The dead `pubsub_conn` is dropped when this
                        // iteration ends.
                        reconnecting = true;

                        let delay = backoff.next().expect("backoff never ends");
                        tokio::select! {
                            _ = tokio::time::sleep(delay) => continue,
                            _ = &mut shutdown => break,
                        }
                    }
                }
            }

            Ok(())
        }))
    }
}
