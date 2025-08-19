use super::config::AmqpSinkConfig;
use super::service::AmqpError;
use crate::amqp::AmqpConfig;
use deadpool::managed::Pool;
use lapin::options::ConfirmSelectOptions;

pub type AmqpSinkChannels = Pool<AmqpSinkChannelManager>;

pub(super) fn new_channel_pool(config: &AmqpSinkConfig) -> crate::Result<AmqpSinkChannels> {
    let max_channels = config.max_channels.try_into().map_err(|_| {
        Box::new(AmqpError::PoolError {
            error: "max_channels must fit into usize".into(),
        })
    })?;
    if max_channels == 0 {
        return Err(Box::new(AmqpError::PoolError {
            error: "max_channels must be positive".into(),
        }));
    }
    let channel_manager = AmqpSinkChannelManager::new(&config.connection);
    let channels = Pool::builder(channel_manager)
        .max_size(max_channels)
        .runtime(deadpool::Runtime::Tokio1)
        .build()?;
    debug!("AMQP channel pool created with max size: {}", max_channels);
    Ok(channels)
}

/// A channel pool manager for the AMQP sink.
/// This manager is responsible for creating and recycling AMQP channels.
/// It uses the `deadpool` crate to manage the channels.
pub(crate) struct AmqpSinkChannelManager {
    config: AmqpConfig,
}

impl deadpool::managed::Manager for AmqpSinkChannelManager {
    type Type = lapin::Channel;
    type Error = AmqpError;

    async fn create(&self) -> Result<Self::Type, Self::Error> {
        let channel = Self::new_channel(&self.config).await?;
        info!(
            message = "Created a new channel to the AMQP broker.",
            id = channel.id(),
            internal_log_rate_limit = true,
        );
        Ok(channel)
    }

    async fn recycle(
        &self,
        channel: &mut Self::Type,
        _: &deadpool::managed::Metrics,
    ) -> deadpool::managed::RecycleResult<Self::Error> {
        let state = channel.status().state();
        if state == lapin::ChannelState::Connected {
            Ok(())
        } else {
            Err((AmqpError::ChannelClosed { state }).into())
        }
    }
}

impl AmqpSinkChannelManager {
    /// Creates a new channel pool manager for the AMQP sink.
    pub fn new(config: &AmqpConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    /// Creates a new AMQP channel using the configuration of this sink.
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
