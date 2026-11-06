use std::{num::NonZeroUsize, time::Duration};

use futures::FutureExt;
use redis::{
    AsyncCommands, ErrorKind, FromRedisValue, RedisError, RedisResult, Value,
    aio::ConnectionManager,
};
use snafu::{ResultExt, Snafu};

use super::{InputHandler, ListOption, Method};
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
    pub(super) async fn watch(mut self, list: ListOption) -> crate::Result<Source> {
        let mut conn = self
            .client
            .get_connection_manager()
            .await
            .context(ConnectionSnafu {})?;
        let mut popper = ListPopper::new(self.key.clone(), list.method, list.batch_size);

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
                let (res, mut stopping) =
                    read_or_shutdown(popper.pop_first(&mut conn), &mut shutdown).await;
                let first = match res {
                    // Shutdown was signalled and the pop did not return in time, e.g. because
                    // the connection is half-open.
                    None => break,
                    // Timed out with the list empty.
                    Some(Ok(None)) => {
                        if stopping {
                            break;
                        }
                        continue;
                    }
                    Some(Ok(Some(first))) => first,
                    Some(Err(error)) => {
                        if is_io_error_after_reporting(error) && !stopping {
                            // Back off before retrying, but stay responsive to shutdown.
                            let delay = backoff.next().expect("backoff never ends");
                            tokio::select! {
                                () = tokio::time::sleep(delay) => {}
                                _ = &mut shutdown => break,
                            }
                        }
                        if stopping {
                            break;
                        }
                        continue;
                    }
                };
                backoff.reset();

                let mut lines = vec![first];
                let mut drain_error = None;
                // No more entries are read once shutdown has been signalled.
                if !stopping {
                    // The counted pop's reply grows with `batch_size`, so it is not cut off by the
                    // blocking pop's grace period.
                    let (res, drain_stopping) =
                        read_or_shutdown_within(popper.drain(&mut conn), &mut shutdown, None).await;
                    stopping = drain_stopping;
                    match res {
                        Some(Ok(rest)) => lines.extend(rest),
                        Some(Err(error)) => drain_error = Some(error),
                        // Shutdown had already been signalled, so the drain was not started.
                        None => {}
                    }
                }

                if let Err(()) = self.handle_lines(lines).await {
                    break;
                }

                if let Some(error) = drain_error
                    && is_io_error_after_reporting(error)
                    && !stopping
                {
                    let delay = backoff.next().expect("backoff never ends");
                    tokio::select! {
                        () = tokio::time::sleep(delay) => {}
                        _ = &mut shutdown => break,
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

/// Emits the error and returns whether it was an I/O error, which warrants a backoff.
fn is_io_error_after_reporting(error: RedisError) -> bool {
    let kind = error.kind();
    emit!(RedisReceiveEventError::from(error));
    kind == ErrorKind::IoError
}

/// How the rest of a batch is read after the first, blocking pop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DrainMode {
    /// `LPOP key count` / `RPOP key count`: one command per batch (Redis 6.2+).
    Count,
    /// The server rejected the `count` argument (older than Redis 6.2), so each batch is just
    /// the blocking pop.
    Unsupported,
}

/// Reads batches of entries from a Redis list, preserving list order.
pub(super) struct ListPopper {
    key: String,
    method: Method,
    batch_size: NonZeroUsize,
    drain_mode: DrainMode,
}

impl ListPopper {
    pub(super) const fn new(key: String, method: Method, batch_size: NonZeroUsize) -> Self {
        Self {
            key,
            method,
            batch_size,
            drain_mode: DrainMode::Count,
        }
    }

    /// Waits until an entry is available and pops it, or returns `None` after
    /// `BLOCKING_POP_TIMEOUT_SECS` if the list stays empty.
    async fn pop_first(&self, conn: &mut ConnectionManager) -> RedisResult<Option<String>> {
        match self.method {
            Method::Rpop => brpop(conn, &self.key).await,
            Method::Lpop => blpop(conn, &self.key).await,
        }
    }

    /// Pops up to `batch_size - 1` more entries without waiting.
    async fn drain(&mut self, conn: &mut ConnectionManager) -> RedisResult<Vec<String>> {
        let Some(count) = NonZeroUsize::new(self.batch_size.get() - 1) else {
            return Ok(Vec::new());
        };

        if self.drain_mode == DrainMode::Unsupported {
            return Ok(Vec::new());
        }

        match pop_count(conn, &self.key, self.method, count).await {
            Err(error) if is_pop_count_unsupported(&error) => {
                warn!(
                    message = "Redis server does not support popping multiple list entries in one command (requires Redis 6.2 or newer); reading one entry at a time.",
                    key = %self.key,
                );
                // The rejected command removed nothing.
                self.drain_mode = DrainMode::Unsupported;
                Ok(Vec::new())
            }
            result => result,
        }
    }
}

async fn pop_count(
    conn: &mut ConnectionManager,
    key: &str,
    method: Method,
    count: NonZeroUsize,
) -> RedisResult<Vec<String>> {
    // A nil reply means the list is empty.
    let popped: Option<Vec<Value>> = match method {
        Method::Lpop => conn.lpop(key, Some(count)).await?,
        Method::Rpop => conn.rpop(key, Some(count)).await?,
    };
    // Convert entries one by one: Redis has already removed all of them, so an entry that is not
    // valid UTF-8 is reported and dropped on its own, as it would be without batching.
    Ok(popped
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| {
            String::from_owned_redis_value(value)
                .map_err(|error| emit!(RedisReceiveEventError::from(error)))
                .ok()
        })
        .collect())
}

/// Servers older than Redis 6.2 reject the `count` argument of `LPOP`/`RPOP` without popping.
fn is_pop_count_unsupported(error: &RedisError) -> bool {
    error.kind() == ErrorKind::ResponseError
        && error.to_string().contains("wrong number of arguments")
}

/// How long a blocking pop waits for an entry before returning, which bounds how long shutdown
/// waits for an in-flight pop on a healthy connection.
const BLOCKING_POP_TIMEOUT_SECS: f64 = 1.0;

/// How long shutdown waits for an in-flight blocking pop to finish. A healthy server replies within
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
    read_or_shutdown_within(read, shutdown, Some(SHUTDOWN_READ_GRACE)).await
}

/// Like `read_or_shutdown`, but with the grace period as a parameter. `None` lets an in-flight read
/// finish however long its reply takes, for replies that grow with `batch_size`; the topology's
/// shutdown deadline still stops the source if the connection never replies.
async fn read_or_shutdown_within<T>(
    read: impl Future<Output = RedisResult<T>>,
    shutdown: &mut ShutdownSignal,
    grace: Option<Duration>,
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
        _ = shutdown => {
            let res = match grace {
                Some(grace) => tokio::time::timeout(grace, read).await.ok(),
                None => Some(read.await),
            };
            (res, true)
        }
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
pub(super) mod integration_test {
    use std::num::NonZeroUsize;

    use redis::AsyncCommands;

    use super::*;
    use crate::test_util::random_string;

    fn redis_url() -> String {
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/0".to_owned())
    }

    async fn connection() -> ConnectionManager {
        redis::Client::open(redis_url())
            .unwrap()
            .get_connection_manager()
            .await
            .unwrap()
    }

    /// Pushes "1".."=count" to a fresh list and returns its key.
    async fn list_with_entries(conn: &mut ConnectionManager, count: usize) -> String {
        let key = format!("test-key-{}", random_string(10));
        for i in 1..=count {
            let _: i64 = conn.rpush(&key, i.to_string()).await.unwrap();
        }
        key
    }

    pub(in crate::sources::redis) async fn server_supports_pop_count(
        conn: &mut ConnectionManager,
    ) -> bool {
        let info: String = redis::cmd("INFO")
            .arg("server")
            .query_async(conn)
            .await
            .unwrap();
        let version = info
            .lines()
            .find_map(|line| line.strip_prefix("redis_version:"))
            .unwrap()
            .trim();
        let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap());
        let (major, minor) = (parts.next().unwrap(), parts.next().unwrap());
        (major, minor) >= (6, 2)
    }

    const fn batch(n: usize) -> NonZeroUsize {
        NonZeroUsize::new(n).unwrap()
    }

    async fn pop_one_batch(popper: &mut ListPopper, conn: &mut ConnectionManager) -> Vec<String> {
        let first = popper
            .pop_first(conn)
            .await
            .unwrap()
            .expect("list is not empty");
        let mut lines = vec![first];
        lines.extend(popper.drain(conn).await.unwrap());
        lines
    }

    #[tokio::test]
    async fn blocking_pop_of_empty_list_times_out() {
        for method in [Method::Lpop, Method::Rpop] {
            let mut conn = connection().await;
            let key = format!("test-key-{}", random_string(10));
            let popper = ListPopper::new(key.clone(), method, batch(1));

            let started = std::time::Instant::now();
            assert_eq!(popper.pop_first(&mut conn).await.unwrap(), None);
            assert!(started.elapsed() >= Duration::from_millis(900));

            // The source keeps reading after a timeout.
            let _: i64 = conn.rpush(&key, "1").await.unwrap();
            assert_eq!(
                popper.pop_first(&mut conn).await.unwrap(),
                Some("1".to_owned())
            );
        }
    }

    #[tokio::test]
    async fn batch_larger_than_list_returns_all_entries() {
        let mut conn = connection().await;
        // Batches beyond the blocking pop need Redis 6.2+; older servers are covered by
        // `drain_mode_follows_server_support_for_pop_count`.
        if !server_supports_pop_count(&mut conn).await {
            return;
        }
        let key = list_with_entries(&mut conn, 3).await;
        let mut popper = ListPopper::new(key.clone(), Method::Lpop, batch(100));

        assert_eq!(pop_one_batch(&mut popper, &mut conn).await, ["1", "2", "3"]);
        let remaining: usize = conn.llen(&key).await.unwrap();
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn drain_keeps_valid_entries_next_to_invalid_utf8() {
        let mut conn = connection().await;
        if !server_supports_pop_count(&mut conn).await {
            return;
        }
        let key = format!("test-key-{}", random_string(10));
        let _: i64 = conn.rpush(&key, "1").await.unwrap();
        let _: i64 = conn.rpush(&key, &[0xff_u8, 0xfe][..]).await.unwrap();
        let _: i64 = conn.rpush(&key, "3").await.unwrap();
        let mut popper = ListPopper::new(key.clone(), Method::Lpop, batch(3));

        let first = popper.pop_first(&mut conn).await.unwrap();
        assert_eq!(first, Some("1".to_owned()));
        // Only the entry that is not valid UTF-8 is dropped, as it would be without batching.
        let rest = popper.drain(&mut conn).await;
        assert!(rest.is_ok(), "drain failed for the whole batch: {rest:?}");
        assert_eq!(rest.unwrap(), ["3"]);
        let remaining: usize = conn.llen(&key).await.unwrap();
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn drain_of_empty_list_returns_no_entries() {
        let mut conn = connection().await;
        let key = list_with_entries(&mut conn, 1).await;
        let mut popper = ListPopper::new(key.clone(), Method::Lpop, batch(10));

        assert_eq!(pop_one_batch(&mut popper, &mut conn).await, ["1"]);
    }

    #[tokio::test]
    async fn drain_mode_follows_server_support_for_pop_count() {
        let mut conn = connection().await;
        let key = list_with_entries(&mut conn, 10).await;
        let mut popper = ListPopper::new(key.clone(), Method::Lpop, batch(5));

        if server_supports_pop_count(&mut conn).await {
            assert_eq!(
                pop_one_batch(&mut popper, &mut conn).await,
                ["1", "2", "3", "4", "5"]
            );
            assert_eq!(popper.drain_mode, DrainMode::Count);
        } else {
            // The rejected probe removes nothing, and later batches are just the blocking pop:
            // no further pops are sent for entries that may not be queued.
            assert_eq!(pop_one_batch(&mut popper, &mut conn).await, ["1"]);
            assert_eq!(popper.drain_mode, DrainMode::Unsupported);
            assert_eq!(pop_one_batch(&mut popper, &mut conn).await, ["2"]);
            let remaining: usize = conn.llen(&key).await.unwrap();
            assert_eq!(remaining, 8);
        }
    }

    #[tokio::test]
    async fn batch_size_one_does_not_drain() {
        let mut conn = connection().await;
        let key = list_with_entries(&mut conn, 3).await;
        let mut popper = ListPopper::new(key.clone(), Method::Lpop, batch(1));

        assert_eq!(pop_one_batch(&mut popper, &mut conn).await, ["1"]);
        let remaining: usize = conn.llen(&key).await.unwrap();
        assert_eq!(remaining, 2);
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
