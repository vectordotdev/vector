use super::{handle_line, ConnectionInfo, Method};
use crate::{
    codecs, config::SourceContext, internal_events::RedisReceiveEventError, sources::Source,
};
use redis::{aio::ConnectionManager, AsyncCommands, RedisResult};
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to create connection: {}", source))]
    Connection { source: redis::RedisError },
}

pub async fn watch(
    client: redis::Client,
    connection_info: ConnectionInfo,
    key: String,
    redis_key: Option<String>,
    method: Method,
    decoder: codecs::Decoder,
    cx: SourceContext,
) -> crate::Result<Source> {
    let mut conn = client
        .get_tokio_connection_manager()
        .await
        .context(ConnectionSnafu {})?;

    Ok(Box::pin(async move {
        let mut shutdown = cx.shutdown;
        let mut tx = cx.out;
        loop {
            let res = match method {
                Method::Rpop => tokio::select! {
                    res = brpop(&mut conn, &key) => res,
                    _ = &mut shutdown => break
                },
                Method::Lpop => tokio::select! {
                    res = blpop(&mut conn, &key) => res,
                    _ = &mut shutdown => break
                },
            };

            match res {
                Err(error) => emit!(RedisReceiveEventError::from(error)),
                Ok(line) => {
                    if let Err(()) = handle_line(
                        &connection_info,
                        line,
                        &key,
                        redis_key.as_deref(),
                        decoder.clone(),
                        &mut tx,
                    )
                    .await
                    {
                        break;
                    }
                }
            }
        }
        Ok(())
    }))
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
