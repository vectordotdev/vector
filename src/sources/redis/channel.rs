use crate::{
    internal_events::{RedisEventReceived, RedisReceiveEventFailed, StreamClosedError},
    shutdown::ShutdownSignal,
    sources::redis::create_event,
    sources::Source,
    SourceSender,
};
use futures::StreamExt;
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to create connection : {}", source))]
    Connection { source: redis::RedisError },
    #[snafu(display("Failed to subscribe to channel: {}", source))]
    Subscribe { source: redis::RedisError },
}

pub async fn subscribe(
    client: redis::Client,
    key: String,
    redis_key: Option<String>,
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
    trace!(message = "Subscribing to channel.", key = key.as_str());
    pubsub_conn
        .subscribe(key.as_str())
        .await
        .context(SubscribeSnafu {})?;
    trace!(message = "Subscribed to channel.", key = key.as_str());
    let fut = async move {
        let mut pubsub_stream = pubsub_conn.on_message().take_until(shutdown.clone());
        while let Some(msg) = pubsub_stream.next().await {
            match msg.get_payload::<String>() {
                Err(error) => emit!(&RedisReceiveEventFailed { error }),
                Ok(line) => {
                    emit!(&RedisEventReceived {
                        byte_size: line.len()
                    });
                    let event = create_event(line.as_str(), key.clone(), &redis_key);
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
