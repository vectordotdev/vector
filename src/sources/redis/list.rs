use std::time::Duration;

use futures::FutureExt;
use redis::{AsyncCommands, ErrorKind, RedisError, RedisResult, aio::ConnectionManager};
use snafu::{ResultExt, Snafu};

use super::{InputHandler, Method};
use crate::{
    common::backoff::ExponentialBackoff, internal_events::RedisReceiveEventError, sources::Source,
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
                // Shutdown is checked between pops instead of racing an in-flight pop: Redis
                // removes an entry as soon as it replies, so abandoning a pop could lose an
                // entry. The blocking pop times out regularly so shutdown is still noticed.
                if (&mut shutdown).now_or_never().is_some() {
                    break;
                }

                let res = match method {
                    Method::Rpop => brpop(&mut conn, &self.key).await,
                    Method::Lpop => blpop(&mut conn, &self.key).await,
                };

                match res {
                    // Timed out with the list empty.
                    Ok(None) => {}
                    Err(error) => {
                        let err: RedisError = error;
                        let kind = err.kind();

                        emit!(RedisReceiveEventError::from(err));

                        if kind == ErrorKind::IoError {
                            // Back off before retrying, but stay responsive to shutdown.
                            let delay = backoff.next().expect("backoff never ends");
                            tokio::select! {
                                _ = tokio::time::sleep(delay) => {}
                                _ = &mut shutdown => break,
                            }
                        }
                    }
                    Ok(Some(line)) => {
                        backoff.reset();
                        if let Err(()) = self.handle_line(line).await {
                            break;
                        }
                    }
                }
            }
            Ok(())
        }))
    }
}

/// How long a blocking pop waits for an entry before returning, which bounds how long shutdown
/// waits for an idle source.
const BLOCKING_POP_TIMEOUT_SECS: f64 = 1.0;

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
