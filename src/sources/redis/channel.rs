use futures_util::StreamExt;
use snafu::{ResultExt, Snafu};

use crate::{
    internal_events::RedisReceiveEventError,
    sources::{
        redis::{ConnectionInfo, InputHandler},
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

impl InputHandler {
    pub(super) async fn subscribe(
        mut self,
        connection_info: ConnectionInfo,
    ) -> crate::Result<Source> {
        let conn = self
            .client
            .get_async_connection()
            .await
            .context(ConnectionSnafu {})?;

        trace!(endpoint = %connection_info.endpoint.as_str(), "Connected.");

        let mut pubsub_conn = conn.into_pubsub();
        pubsub_conn
            .subscribe(&self.key)
            .await
            .context(SubscribeSnafu {})?;
        trace!(endpoint = %connection_info.endpoint.as_str(), channel = %self.key, "Subscribed to channel.");

        Ok(Box::pin(async move {
            let shutdown = self.cx.shutdown.clone();
            let mut pubsub_stream = pubsub_conn.on_message().take_until(shutdown);
            while let Some(msg) = pubsub_stream.next().await {
                match msg.get_payload::<String>() {
                    Ok(line) => {
                        if let Err(()) = self.handle_line(line).await {
                            break;
                        }
                    }
                    Err(error) => emit!(RedisReceiveEventError::from(error)),
                }
            }
            Ok(())
        }))
    }
}
