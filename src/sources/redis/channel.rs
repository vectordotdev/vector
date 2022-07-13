use futures_util::StreamExt;
use snafu::{ResultExt, Snafu};

use crate::{
    codecs,
    config::SourceContext,
    internal_events::RedisReceiveEventError,
    sources::{
        redis::{handle_line, ConnectionInfo},
        Source,
    },
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to create connection: {}", source))]
    Connection { source: redis::RedisError },
    #[snafu(display("Failed to subscribe to channel: {}", source))]
    Subscribe { source: redis::RedisError },
}

pub async fn subscribe(
    client: redis::Client,
    connection_info: ConnectionInfo,
    key: String,
    redis_key: Option<String>,
    decoder: codecs::Decoder,
    cx: SourceContext,
) -> crate::Result<Source> {
    let conn = client
        .get_async_connection()
        .await
        .context(ConnectionSnafu {})?;
    trace!(endpoint = %connection_info.endpoint.as_str(), "Connected.");

    let mut pubsub_conn = conn.into_pubsub();
    pubsub_conn
        .subscribe(&key)
        .await
        .context(SubscribeSnafu {})?;
    trace!(endpoint = %connection_info.endpoint.as_str(), channel = %key, "Subscribed to channel.");

    Ok(Box::pin(async move {
        let shutdown = cx.shutdown;
        let mut tx = cx.out;
        let mut pubsub_stream = pubsub_conn.on_message().take_until(shutdown);
        while let Some(msg) = pubsub_stream.next().await {
            match msg.get_payload::<String>() {
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
                Err(error) => emit!(RedisReceiveEventError::from(error)),
            }
        }
        Ok(())
    }))
}
