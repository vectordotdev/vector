use crate::amqp::AmqpConfig;
use lapin::options::ConfirmSelectOptions;
use tokio::sync::{RwLock, RwLockReadGuard};

use super::service::AmqpError;

/// A wrapper around the AMQP channel that handles reconnections.
pub(crate) struct AmqpChannel {
    channel: RwLock<lapin::Channel>,
    config: AmqpConfig,
}

impl AmqpChannel {
    /// Creates a new AMQP channel.
    pub async fn new(config: &AmqpConfig) -> Result<Self, AmqpError> {
        let channel = Self::new_channel(config).await?;

        Ok(Self {
            channel: RwLock::new(channel),
            config: config.clone(),
        })
    }

    /// Returns a read lock to the AMQP channel. If the current channel is in an error state,
    /// it will attempt to reconnect and create a new channel.
    pub async fn channel(&self) -> Result<RwLockReadGuard<'_, lapin::Channel>, AmqpError> {
        let need_reconnect =
            { self.channel.read().await.status().state() == lapin::ChannelState::Error };

        if need_reconnect {
            let mut channel = self.channel.write().await;

            // Check if we still need to reconnect after acquiring the write lock.
            if channel.status().state() != lapin::ChannelState::Error {
                return Ok(channel.downgrade());
            }

            info!(
                message = "Recovering broken connection to the AMQP broker.",
                internal_log_rate_limit = true,
            );

            *channel = Self::new_channel(&self.config).await?;

            info!(
                message = "Recovered connection to the AMQP broker.",
                internal_log_rate_limit = true,
            );
        }
        Ok(self.channel.read().await)
    }

    async fn new_channel(config: &AmqpConfig) -> Result<lapin::Channel, AmqpError> {
        let (_, channel) = config
            .connect()
            .await
            .map_err(|e| AmqpError::ConnectFailed { error: e })?;

        // Enable confirmations on the channel.
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await
            .map_err(|e| AmqpError::ConnectFailed { error: Box::new(e) })?;

        Ok(channel)
    }
}
