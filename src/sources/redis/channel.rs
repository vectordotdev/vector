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

/// How long a pub/sub session must stay connected before we consider it healthy and reset
/// the reconnect backoff, even if it hasn't delivered any messages. This keeps a flapping
/// connection backing off while ensuring a stable-but-quiet low-volume channel doesn't retain
/// a backoff that a previous flapping period drove up to the cap.
const HEALTHY_SESSION_THRESHOLD: Duration = Duration::from_secs(60);

/// Whether a Redis error is transient (worth reconnecting) rather than non-recoverable.
///
/// Uses redis-rs's own retry classification (`RetryMethod::NoRetry` => permanent) with one
/// override: redis-rs maps `ErrorKind::AuthenticationFailed` to `RetryMethod::Reconnect`, but
/// bad credentials/permissions are never fixed by reconnecting, so we treat them as permanent
/// too. This makes an invalid configuration (auth/ACL/config) fail fast instead of retrying
/// forever, while everything else — I/O failures (unreachable/reset/timeout) and server-side
/// retryable states such as `LOADING`/`BUSYLOADING`/`TRYAGAIN` during a Redis restart — is
/// transient and reconnected.
fn is_transient(error: &redis::RedisError) -> bool {
    if error.kind() == redis::ErrorKind::AuthenticationFailed {
        return false;
    }
    !matches!(error.retry_method(), redis::RetryMethod::NoRetry)
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
    /// The initial connect + SUBSCRIBE happens here so a non-recoverable error (bad auth,
    /// ACLs, invalid config) fails the source build immediately. Once started, the source
    /// runs a reconnect loop: on a dropped connection it reconnects with exponential
    /// backoff instead of stopping, so a Redis restart or transient network blip no longer
    /// requires a manual Vector restart.
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
            backoff: &mut ExponentialBackoff,
        ) -> SessionEnd
        where
            S: std::future::Future + Unpin,
        {
            let mut stream = pubsub_conn.on_message();

            // Once the connection has either delivered a message or simply stayed up for
            // `HEALTHY_SESSION_THRESHOLD`, we consider it healthy and reset the backoff. The
            // timer covers low-volume channels that stay connected a long time without
            // publishing, so a stable-but-quiet session doesn't keep a backoff a prior
            // flapping period drove up to the cap.
            let healthy = tokio::time::sleep(HEALTHY_SESSION_THRESHOLD);
            tokio::pin!(healthy);
            let mut backoff_reset = false;

            loop {
                // One "step" in the session: either we got a message, the connection became
                // healthy, Redis dropped us, or shutdown fired.
                enum RecvEvent {
                    Msg(redis::Msg),
                    Healthy,
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
                    _ = &mut healthy, if !backoff_reset => RecvEvent::Healthy,
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
                            // A message was delivered downstream: the connection is healthy,
                            // so reset the reconnect backoff. Resetting only on a health signal
                            // (data, or the timer below) — never on a bare connect — means a
                            // connection that drops before becoming healthy keeps backing off.
                            if !backoff_reset {
                                backoff.reset();
                                backoff_reset = true;
                            }
                        }
                        Err(error) => {
                            // Bad payload. We just log and keep going.
                            emit!(RedisReceiveEventError::from(error));
                        }
                    },

                    RecvEvent::Healthy => {
                        // Stayed connected long enough to be considered stable even without
                        // delivering data (low-volume channel): reset the backoff.
                        backoff.reset();
                        backoff_reset = true;
                    }

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

        // Initial connect + SUBSCRIBE, performed here (not inside the source future) so a
        // non-recoverable error fails the source build immediately rather than silently
        // entering a retry loop that makes an invalid config look like it started. A
        // transient error (Redis unreachable/restarting) is tolerated and handled by the
        // reconnect loop once the source starts.
        let initial_conn = match connect_and_subscribe(&client, &endpoint, &channel).await {
            Ok(conn) => {
                emit!(RedisConnectionEstablished { reconnect: false });
                Some(conn)
            }
            Err(err) => {
                let source = err.into_source();
                if !is_transient(&source) {
                    return Err(source.into());
                }
                emit!(RedisConnectionError::from(source));
                None
            }
        };

        Ok(Box::pin(async move {
            // `shutdown` is a signal that resolves when Vector is stopping.
            let mut shutdown = self.cx.shutdown.clone();

            // Exponential backoff between reconnect attempts: 500ms, 1s, 2s, 4s, ...
            // capped at 30s. Matches the strategy used by other reconnecting sources
            // (e.g. `aws_s3`/`sqs`). Only reset once a session delivers data (see
            // `run_subscription_session`), so a flapping connection still backs off.
            let mut backoff = ExponentialBackoff::from_millis(2)
                .factor(250)
                .max_delay(Duration::from_secs(30));

            // A connection to run next: the one established at startup, or `None` to
            // (re)connect via the loop below.
            let mut next_conn = initial_conn;

            loop {
                // Obtain a live connection: reuse the pending one, or reconnect with backoff.
                let mut pubsub_conn = match next_conn.take() {
                    Some(conn) => conn,
                    None => 'reconnect: loop {
                        let delay = backoff.next().expect("backoff never ends");
                        tokio::select! {
                            _ = tokio::time::sleep(delay) => {}
                            _ = &mut shutdown => return Ok(()),
                        }

                        // Race the connect with shutdown: a connect against a black-holed
                        // endpoint can stall for a long time, and we must still observe
                        // shutdown promptly instead of waiting for the force-shutdown
                        // deadline. `biased` makes shutdown win when both are ready.
                        let res = tokio::select! {
                            biased;
                            _ = &mut shutdown => return Ok(()),
                            res = connect_and_subscribe(&client, &endpoint, &channel) => res,
                        };

                        match res {
                            Ok(conn) => {
                                emit!(RedisConnectionEstablished { reconnect: true });
                                break 'reconnect conn;
                            }
                            Err(err) => {
                                let source = err.into_source();
                                let permanent = !is_transient(&source);
                                emit!(RedisConnectionError::from(source));
                                if permanent {
                                    // Non-recoverable (auth/ACL/config): stop the source
                                    // rather than retry forever.
                                    return Err(());
                                }
                                // transient: keep retrying; backoff advances next iteration.
                            }
                        }
                    },
                };

                // run that session (receive messages, forward them, etc.)
                let end_reason = run_subscription_session(
                    &mut pubsub_conn,
                    &channel,
                    &mut shutdown,
                    &mut self,
                    &endpoint,
                    &mut backoff,
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
                        // Redis dropped us. `next_conn` stays `None`, so the next iteration
                        // reconnects with backoff. The dead `pubsub_conn` is dropped when
                        // this iteration ends.
                    }
                }
            }

            Ok(())
        }))
    }
}
