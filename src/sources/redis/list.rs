use redis::{aio::ConnectionManager, AsyncCommands, RedisResult};
use snafu::{ResultExt, Snafu};
use vector_common::internal_event::{BytesReceived, Registered};
use vector_core::config::LogNamespace;

use super::{handle_line, Method};
use crate::{
    codecs, config::SourceContext, internal_events::RedisReceiveEventError, sources::Source,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to create connection: {}", source))]
    Connection { source: redis::RedisError },
}

pub struct WatchInputs {
    pub client: redis::Client,
    pub bytes_received: Registered<BytesReceived>,
    pub key: String,
    pub redis_key: Option<String>,
    pub method: Method,
    pub decoder: codecs::Decoder,
    pub cx: SourceContext,
    pub log_namespace: LogNamespace,
}

pub async fn watch(input: WatchInputs) -> crate::Result<Source> {
    let mut conn = input
        .client
        .get_tokio_connection_manager()
        .await
        .context(ConnectionSnafu {})?;

    Ok(Box::pin(async move {
        let mut shutdown = input.cx.shutdown;
        let mut tx = input.cx.out;
        loop {
            let res = match input.method {
                Method::Rpop => tokio::select! {
                    res = brpop(&mut conn, &input.key) => res,
                    _ = &mut shutdown => break
                },
                Method::Lpop => tokio::select! {
                    res = blpop(&mut conn, &input.key) => res,
                    _ = &mut shutdown => break
                },
            };

            match res {
                Err(error) => emit!(RedisReceiveEventError::from(error)),
                Ok(line) => {
                    if let Err(()) = handle_line(
                        line,
                        &input.key,
                        input.redis_key.as_deref(),
                        input.decoder.clone(),
                        &input.bytes_received,
                        &mut tx,
                        input.log_namespace,
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
