use futures_util::StreamExt;
use snafu::{ResultExt, Snafu};

use crate::{
    internal_events::RedisReceiveEventError,
    sources::{
        Source,
        redis::{ConnectionInfo, InputHandler},
    },
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Failed to create connection: {}", source))]
    Connection { source: redis::RedisError },
    #[snafu(display("Failed to subscribe to channel: {}", source))]
    Subscribe { source: redis::RedisError },
}

impl InputHandler {
    pub(super) async fn subscribe(self, connection_info: ConnectionInfo) -> crate::Result<Source> {
        let mut pubsub_conn = self
            .client
            .get_async_pubsub()
            .await
            .context(ConnectionSnafu {})?;

        trace!(endpoint = %connection_info.endpoint.as_str(), "Connected.");

        pubsub_conn
            .subscribe(&self.key)
            .await
            .context(SubscribeSnafu {})?;
        trace!(endpoint = %connection_info.endpoint.as_str(), channel = %self.key, "Subscribed to channel.");

        Ok(self.run(pubsub_conn, false))
    }

    pub(super) async fn psubscribe(self, connection_info: ConnectionInfo) -> crate::Result<Source> {
        let mut pubsub_conn = self
            .client
            .get_async_pubsub()
            .await
            .context(ConnectionSnafu {})?;

        trace!(endpoint = %connection_info.endpoint.as_str(), "Connected.");

        pubsub_conn
            .psubscribe(&self.key)
            .await
            .context(SubscribeSnafu {})?;
        trace!(endpoint = %connection_info.endpoint.as_str(), pattern = %self.key, "Subscribed to channel with pattern.");

        Ok(self.run(pubsub_conn, true))
    }

    fn run(mut self, mut pubsub_conn: redis::aio::PubSub, with_channel: bool) -> Source {
        Box::pin(async move {
            let shutdown = self.cx.shutdown.clone();
            let mut pubsub_stream = pubsub_conn.on_message().take_until(shutdown);
            while let Some(msg) = pubsub_stream.next().await {
                // For pattern subscriptions, record which concrete channel matched.
                let channel = with_channel.then(|| msg.get_channel_name().to_string());
                match msg.get_payload::<String>() {
                    Ok(line) => {
                        if let Err(()) = self.handle_line(line, channel.as_deref()).await {
                            break;
                        }
                    }
                    Err(error) => emit!(RedisReceiveEventError::from(error)),
                }
            }
            Ok(())
        })
    }
}
