use std::time::Duration;

use futures::FutureExt;
use redis::{AsyncCommands, ErrorKind, RedisError, RedisResult, aio::ConnectionManager};
use snafu::{ResultExt, Snafu};

use super::{InputHandler, Method};
use crate::{
    common::backoff::ExponentialBackoff, internal_events::RedisReceiveEventError,
    shutdown::ShutdownSignal, sources::Source,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to create connection: {}", source))]
    Connection { source: RedisError },
}

impl InputHandler {
    pub(super) async fn watch(mut self, method: Method) -> crate::Result<Source> {
        let mut conn = self
            .client
            .get_connection_manager()
            .await
            .context(ConnectionSnafu {})?;

        Ok(Box::pin(async move {
            let mut shutdown = self.cx.shutdown.clone();

            // Exponential backoff between retries after an I/O error: 500ms, then capped at
            // 1s. Uses the shared `ExponentialBackoff` helper (as the `channel` source does)
            // but keeps this source's prior 1s cap so recovery latency after a Redis outage
            // is unchanged. Reset once a value is successfully received.
            let mut backoff = ExponentialBackoff::from_millis(2)
                .factor(250)
                .max_delay(Duration::from_secs(1));

            loop {
                let pop = async {
                    match method {
                        Method::Rpop => brpop(&mut conn, &self.key).await,
                        Method::Lpop => blpop(&mut conn, &self.key).await,
                    }
                };
                let (res, stopping) = read_or_shutdown(pop, &mut shutdown).await;

                match res {
                    // Shutdown was signalled and the pop did not return in time, e.g. because
                    // the connection is half-open.
                    None => break,
                    // Timed out with the list empty.
                    Some(Ok(None)) => {}
                    Some(Err(error)) => {
                        let err: RedisError = error;
                        let kind = err.kind();

                        emit!(RedisReceiveEventError::from(err));

                        if kind == ErrorKind::IoError && !stopping {
                            // Back off before retrying, but stay responsive to shutdown.
                            let delay = backoff.next().expect("backoff never ends");
                            tokio::select! {
                                () = tokio::time::sleep(delay) => {}
                                _ = &mut shutdown => break,
                            }
                        }
                    }
                    Some(Ok(Some(line))) => {
                        backoff.reset();
                        if let Err(()) = self.handle_line(line).await {
                            break;
                        }
                    }
                }

                if stopping {
                    break;
                }
            }
            Ok(())
        }))
    }
}

/// How long a blocking pop waits for an entry before returning, which bounds how long shutdown
/// waits for an in-flight pop on a healthy connection.
const BLOCKING_POP_TIMEOUT_SECS: f64 = 1.0;

/// How long shutdown waits for an in-flight read to finish. A healthy server replies within
/// `BLOCKING_POP_TIMEOUT_SECS` plus a round trip; this only bounds shutdown when the reply never
/// arrives, so an unresponsive Redis connection cannot stall shutdown.
const SHUTDOWN_READ_GRACE: Duration = Duration::from_secs(5);

/// Awaits a Redis read while watching for shutdown. If shutdown is signalled first, the read is
/// still given `SHUTDOWN_READ_GRACE` to finish: Redis removes entries as soon as it replies, so
/// abandoning a read whose reply is on its way would lose them.
///
/// Returns the read's result, or `None` if it did not finish in time or was not started because
/// shutdown had already been signalled, and whether shutdown was signalled. Once shutdown was
/// signalled, `shutdown` must not be awaited again.
async fn read_or_shutdown<T>(
    read: impl Future<Output = RedisResult<T>>,
    shutdown: &mut ShutdownSignal,
) -> (Option<RedisResult<T>>, bool) {
    // Do not send a new read once shutdown has been signalled.
    if (&mut *shutdown).now_or_never().is_some() {
        return (None, true);
    }

    tokio::pin!(read);
    tokio::select! {
        // Poll the read first so it is sent before shutdown is checked: the grace period then only
        // ever applies to a read that is already in flight.
        biased;
        res = &mut read => (Some(res), false),
        _ = shutdown => (tokio::time::timeout(SHUTDOWN_READ_GRACE, read).await.ok(), true),
    }
}

// A nil reply means the pop timed out.
async fn brpop(conn: &mut ConnectionManager, key: &str) -> RedisResult<Option<String>> {
    conn.brpop(key, BLOCKING_POP_TIMEOUT_SECS)
        .await
        .map(|popped: Option<(String, String)>| popped.map(|(_, value)| value))
}

async fn blpop(conn: &mut ConnectionManager, key: &str) -> RedisResult<Option<String>> {
    conn.blpop(key, BLOCKING_POP_TIMEOUT_SECS)
        .await
        .map(|popped: Option<(String, String)>| popped.map(|(_, value)| value))
}

#[cfg(all(test, feature = "redis-integration-tests"))]
mod integration_test {
    use super::*;
    use crate::test_util::random_string;

    #[tokio::test]
    async fn blocking_pop_of_empty_list_times_out() {
        let url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/0".to_owned());
        let mut conn = redis::Client::open(url)
            .unwrap()
            .get_connection_manager()
            .await
            .unwrap();

        for method in [Method::Lpop, Method::Rpop] {
            let key = format!("test-key-{}", random_string(10));

            let started = std::time::Instant::now();
            assert_eq!(pop(method, &mut conn, &key).await.unwrap(), None);
            assert!(started.elapsed() >= Duration::from_millis(900));

            // The source keeps reading after a timeout.
            let _: i64 = conn.rpush(&key, "1").await.unwrap();
            assert_eq!(
                pop(method, &mut conn, &key).await.unwrap(),
                Some("1".to_owned())
            );
        }
    }

    async fn pop(
        method: Method,
        conn: &mut ConnectionManager,
        key: &str,
    ) -> RedisResult<Option<String>> {
        match method {
            Method::Lpop => blpop(conn, key).await,
            Method::Rpop => brpop(conn, key).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[tokio::test]
    async fn read_is_not_started_after_shutdown() {
        let (trigger, mut shutdown, _done) = ShutdownSignal::new_wired();
        trigger.cancel();

        let started = AtomicBool::new(false);
        let read = async {
            started.store(true, Ordering::SeqCst);
            RedisResult::Ok(())
        };

        let (res, stopping) = read_or_shutdown(read, &mut shutdown).await;
        assert!(res.is_none());
        assert!(stopping);
        assert!(!started.load(Ordering::SeqCst), "a new read was sent");
    }

    #[tokio::test]
    async fn in_flight_read_finishes_after_shutdown() {
        let (trigger, mut shutdown, _done) = ShutdownSignal::new_wired();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let read = async move { RedisResult::Ok(reply_rx.await.unwrap()) };

        // The read is waiting for its reply when shutdown is signalled; the reply then arrives.
        let signal_then_reply = async move {
            tokio::task::yield_now().await;
            trigger.cancel();
            tokio::task::yield_now().await;
            reply_tx.send(42).unwrap();
        };
        let ((res, stopping), ()) =
            tokio::join!(read_or_shutdown(read, &mut shutdown), signal_then_reply);

        assert_eq!(res.unwrap().unwrap(), 42);
        assert!(stopping);
    }
}
