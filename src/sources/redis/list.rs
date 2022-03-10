use super::{handle_line, Method};
use crate::{
    codecs, internal_events::RedisReceiveEventFailed, shutdown::ShutdownSignal, sources::Source,
    SourceSender,
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
    key: String,
    redis_key: Option<String>,
    method: Method,
    decoder: codecs::Decoder,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> crate::Result<Source> {
    trace!(message = "Get redis connection manager.");
    let mut conn = client
        .get_tokio_connection_manager()
        .await
        .context(ConnectionSnafu {})?;
    trace!(message = "Got redis connection manager.");

    let fut = async move {
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
                Err(error) => emit!(&RedisReceiveEventFailed { error }),
                Ok(line) => {
                    if let Err(()) =
                        handle_line(line, &key, redis_key.as_deref(), decoder.clone(), &mut out)
                            .await
                    {
                        break;
                    }
                }
            }
        }
        Ok(())
    };
    Ok(Box::pin(fut))
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
