use crate::{
    internal_events::{RedisEventReceived, RedisReceiveEventFailed, StreamClosedError},
    shutdown::ShutdownSignal,
    sources::redis::{create_event, Method},
    sources::Source,
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
                    emit!(&RedisEventReceived {
                        byte_size: line.len()
                    });
                    let event = create_event(&line, &key, redis_key.as_deref());
                    if let Err(error) = out.send(event).await {
                        emit!(&StreamClosedError { error, count: 1 });
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
    let res: RedisResult<(String, String)> = conn.brpop(key, 0).await;
    res.map(|(_, value)| value)
}

async fn blpop(conn: &mut ConnectionManager, key: &str) -> RedisResult<String> {
    let res: RedisResult<(String, String)> = conn.blpop(key, 0).await;
    res.map(|(_, value)| value)
}
