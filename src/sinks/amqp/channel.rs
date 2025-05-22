use super::service::AmqpError;
use crate::amqp::AmqpConfig;
use lapin::options::ConfirmSelectOptions;

/// A wrapper around the AMQP channel that handles reconnections.
pub(crate) struct AmqpChannel {
    config: AmqpConfig,
}

impl deadpool::managed::Manager for AmqpChannel {
    type Type = lapin::Channel;
    type Error = AmqpError;

    async fn create(&self) -> Result<Self::Type, Self::Error> {
        let channel = Self::new_channel(&self.config).await?;
        info!(
            message = "Created a new channel to the AMQP broker.",
            id = channel.id()
        );
        Ok(channel)
    }

    async fn recycle(
        &self,
        channel: &mut Self::Type,
        _: &deadpool::managed::Metrics,
    ) -> deadpool::managed::RecycleResult<Self::Error> {
        if channel.status().state() == lapin::ChannelState::Connected {
            Ok(())
        } else {
            Err((AmqpError::ChannelClosed {}).into())
        }
    }
}

impl AmqpChannel {
    /// Creates a new AMQP channel.
    pub fn new(config: &AmqpConfig) -> Self {
        Self {
            config: config.clone(),
        }
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
