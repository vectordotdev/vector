use futures_util::StreamExt;
use snafu::Snafu;
use std::time::Duration;
use tracing::{info, trace, warn};

use crate::{
    internal_events::RedisReceiveEventError,
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

/// Exponential backoff used between reconnect attempts.
async fn backoff_exponential(exp: u32) {
    let ms = if exp <= 4 { 2_u64.pow(exp + 5) } else { 1000 };
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

impl InputHandler {
    /// Build the Redis `channel` source.
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
                            channel, "Redis pubsub stream ended; will reconnect"
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

            // retry counter for exponential backoff between reconnects
            let mut retry: u32 = 0;

            loop {
                // connect + SUBSCRIBE
                let mut pubsub_conn =
                    match connect_and_subscribe(&client, &endpoint, &channel).await {
                        Ok(conn) => {
                            let was_reconnecting = retry > 0;

                            if was_reconnecting {
                                // we previously failed but now we're back
                                info!(
                                    endpoint = %endpoint,
                                    channel  = %channel,
                                    "Redis pubsub connection re-established and resubscribed"
                                );
                            } else {
                                trace!(
                                    endpoint = %endpoint,
                                    channel  = %channel,
                                    "Redis pubsub connection established"
                                );
                            }

                            retry = 0;
                            conn
                        }
                        Err(err) => {
                            // failed to connect or SUBSCRIBE
                            warn!(
                                %err,
                                endpoint = %endpoint,
                                channel  = %channel,
                                "Failed to establish subscription; will retry"
                            );

                            retry += 1;

                            // back off before retrying, unless we're shutting down
                            tokio::select! {
                                _ = backoff_exponential(retry) => {
                                    continue;
                                }
                                _ = &mut shutdown => {
                                    break;
                                }
                            }
                        }
                    };

                // run that session (receive messages, forward them, etc.)
                let end_reason = run_subscription_session(
                    &mut pubsub_conn,
                    &channel,
                    &mut shutdown,
                    &mut self,
                    &endpoint,
                )
                .await;

                match end_reason {
                    SessionEnd::Shutdown => {
                        // shutting down cleanly
                        let _ = pubsub_conn.unsubscribe(&channel).await;
                        break;
                    }

                    SessionEnd::DownstreamClosed => {
                        // downstream closed, no point continuing
                        let _ = pubsub_conn.unsubscribe(&channel).await;
                        break;
                    }

                    SessionEnd::Disconnected => {
                        // Redis dropped us. We'll try to reconnect after a backoff,
                        // unless shutdown fires during that backoff.
                        retry += 1;

                        tokio::select! {
                            _ = backoff_exponential(retry) => {
                                let _ = pubsub_conn.unsubscribe(&channel).await;
                                continue;
                            }
                            _ = &mut shutdown => {
                                let _ = pubsub_conn.unsubscribe(&channel).await;
                                break;
                            }
                        }
                    }
                }
            }

            Ok(())
        }))
    }
}
