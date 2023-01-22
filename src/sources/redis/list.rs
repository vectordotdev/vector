use redis::{aio::ConnectionManager, AsyncCommands, RedisResult};
use snafu::{ResultExt, Snafu};

use super::{InputHandler, Method};
use crate::{internal_events::RedisReceiveEventError, sources::Source};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to create connection: {}", source))]
    Connection { source: redis::RedisError },
}

impl InputHandler {
    pub(super) async fn watch(mut self, method: Method) -> crate::Result<Source> {
        let mut conn = self
            .client
            .get_tokio_connection_manager()
            .await
            .context(ConnectionSnafu {})?;

        Ok(Box::pin(async move {
            let mut shutdown = self.cx.shutdown.clone();
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
                    Err(error) => emit!(RedisReceiveEventError::from(error)),
                    Ok(line) => {
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
    conn.brpop(key, 0)
        .await
        .map(|(_, value): (String, String)| value)
}

async fn blpop(conn: &mut ConnectionManager, key: &str) -> RedisResult<String> {
    conn.blpop(key, 0)
        .await
        .map(|(_, value): (String, String)| value)
}
