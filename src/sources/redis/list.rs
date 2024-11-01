use redis::{aio::ConnectionManager, AsyncCommands, ErrorKind, RedisError, RedisResult};
use snafu::{ResultExt, Snafu};
use std::time::Duration;

use super::{InputHandler, Method};
use crate::{internal_events::RedisReceiveEventError, sources::Source};

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
            let mut retry: u32 = 0;
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
                            retry += 1;
                            backoff_exponential(retry).await
                        }
                    }
                    Ok(line) => {
                        if retry > 0 {
                            retry = 0
                        }
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

async fn backoff_exponential(exp: u32) {
    let ms = if exp <= 4 { 2_u64.pow(exp + 5) } else { 1000 };
    tokio::time::sleep(Duration::from_millis(ms)).await;
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
