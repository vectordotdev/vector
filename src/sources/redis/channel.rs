use futures_util::StreamExt;
use snafu::{ResultExt, Snafu};
use vector_common::internal_event::{BytesReceived, Registered};
use vector_core::config::LogNamespace;

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

pub struct SubscribeInputs {
    pub client: redis::Client,
    pub connection_info: ConnectionInfo,
    pub bytes_received: Registered<BytesReceived>,
    pub key: String,
    pub redis_key: Option<String>,
    pub decoder: codecs::Decoder,
    pub cx: SourceContext,
    pub log_namespace: LogNamespace,
}

pub async fn subscribe(input: SubscribeInputs) -> crate::Result<Source> {
    let conn = input
        .client
        .get_async_connection()
        .await
        .context(ConnectionSnafu {})?;

    trace!(endpoint = %input.connection_info.endpoint.as_str(), "Connected.");

    let mut pubsub_conn = conn.into_pubsub();
    pubsub_conn
        .subscribe(&input.key)
        .await
        .context(SubscribeSnafu {})?;
    trace!(endpoint = %input.connection_info.endpoint.as_str(), channel = %input.key, "Subscribed to channel.");

    Ok(Box::pin(async move {
        let shutdown = input.cx.shutdown;
        let mut tx = input.cx.out;
        let mut pubsub_stream = pubsub_conn.on_message().take_until(shutdown);
        while let Some(msg) = pubsub_stream.next().await {
            match msg.get_payload::<String>() {
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
                Err(error) => emit!(RedisReceiveEventError::from(error)),
            }
        }
        Ok(())
    }))
}
