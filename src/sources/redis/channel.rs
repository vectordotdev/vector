use super::handle_line;
use crate::{
    codecs, internal_events::RedisReceiveEventFailed, shutdown::ShutdownSignal, sources::Source,
    SourceSender,
};
use futures_util::StreamExt;
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to create connection: {}", source))]
    Connection { source: redis::RedisError },
    #[snafu(display("Failed to subscribe to channel: {}", source))]
    Subscribe { source: redis::RedisError },
}

pub async fn subscribe(
    client: redis::Client,
    key: String,
    redis_key: Option<String>,
    decoder: codecs::Decoder,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> crate::Result<Source> {
    trace!(message = "Get redis async connection.");
    let conn = client
        .get_async_connection()
        .await
        .context(ConnectionSnafu {})?;
    trace!(message = "Got redis async connection.");
    let mut pubsub_conn = conn.into_pubsub();
    trace!(message = "Subscribing to channel.", key = %key);
    pubsub_conn
        .subscribe(&key)
        .await
        .context(SubscribeSnafu {})?;
    trace!(message = "Subscribed to channel.", key = %key);
    let fut = async move {
        let mut pubsub_stream = pubsub_conn.on_message().take_until(shutdown.clone());
        while let Some(msg) = pubsub_stream.next().await {
            match msg.get_payload::<String>() {
                Ok(line) => {
                    if let Err(()) =
                        handle_line(line, &key, redis_key.as_deref(), decoder.clone(), &mut out)
                            .await
                    {
                        break;
                    }
                }
                Err(error) => emit!(&RedisReceiveEventFailed { error }),
            }
        }
        Ok(())
    };
    Ok(Box::pin(fut))
}
