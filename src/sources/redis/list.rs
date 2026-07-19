use std::time::Duration;

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

            // Exponential backoff between retries after an I/O error: 500ms, 1s, 2s, 4s,
            // ... capped at 30s. Shares the strategy used by the `channel` source and
            // `aws_s3`/`sqs`. Reset once a value is successfully received.
            let mut backoff = ExponentialBackoff::from_millis(2)
                .factor(250)
                .max_delay(Duration::from_secs(30));

            loop {
                let res = match method {
                    Method::Rpop => tokio::select! {
                        res = brpop(&mut conn, &self.key) => res,
                        _ = &mut shutdown => break
                    },
                    Method::Lpop => tokio::select! {
                        res = blpop(&mut conn, &self.key) => res,
                        _ = &mut shutdown => break
                    },
                };

                match res {
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
                    Ok(line) => {
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

async fn brpop(conn: &mut ConnectionManager, key: &str) -> RedisResult<String> {
    conn.brpop(key, 0.0)
        .await
        .map(|(_, value): (String, String)| value)
}

async fn blpop(conn: &mut ConnectionManager, key: &str) -> RedisResult<String> {
    conn.blpop(key, 0.0)
        .await
        .map(|(_, value): (String, String)| value)
}
