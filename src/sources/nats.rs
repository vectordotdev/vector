use crate::{
    config::{log_schema, DataType, SourceConfig, SourceContext, SourceDescription},
    event::{Event, Value},
    internal_events::NatsEventReceived,
    shutdown::ShutdownSignal,
    Pipeline,
};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{collections::BTreeMap, collections::HashMap};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Could not create Nats subscriber: {}", source))]
    NatsCreateError { source: std::io::Error },
    #[snafu(display("Could not subscribe to Nats topics: {}", source))]
    NatsSubscribeError { source: std::io::Error },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NatsSourceConfig {
    url: String,
    name: String,
    subject: String,
    queue: Option<String>,
}

inventory::submit! {
    SourceDescription::new::<NatsSourceConfig>("nats")
}

impl_generate_config_from_default!(NatsSourceConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "nats")]
impl SourceConfig for NatsSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let (connection, subscription) = create_subscription(self).await?;

        Ok(Box::pin(nats_source(
            connection,
            subscription,
            cx.shutdown,
            cx.out,
        )))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "nats"
    }
}

impl NatsSourceConfig {
    fn to_nats_options(&self) -> async_nats::Options {
        // Set reconnect_buffer_size on the nats client to 0 bytes so that the
        // client doesn't buffer internally (to avoid message loss).
        async_nats::Options::new()
            .with_name(&self.name)
            .reconnect_buffer_size(0)
    }

    async fn connect(&self) -> crate::Result<async_nats::Connection> {
        self.to_nats_options()
            .connect(&self.url)
            .await
            .map_err(|e| e.into())
    }
}

impl From<NatsSourceConfig> for async_nats::Options {
    fn from(config: NatsSourceConfig) -> Self {
        async_nats::Options::new()
            .with_name(&config.name)
            .reconnect_buffer_size(0)
    }
}

async fn nats_source(
    _connection: async_nats::Connection,
    subscription: async_nats::Subscription,
    shutdown: ShutdownSignal,
    mut out: Pipeline,
) -> Result<(), ()> {
    while let Some(msg) = subscription.next().await {
        emit!(NatsEventReceived {
            byte_size: msg.data.len(),
        });

        let mut event = Event::new_empty_log();
        let log = event.as_mut_log();

        log.insert(
            log_schema().message_key(),
            Value::from(Bytes::from(msg.data)),
        );

        // Add source type
        log.insert(log_schema().source_type_key(), Bytes::from("nats"));

        match out.send(event).await {
            Err(error) => error!(message = "Error sending to sink.", %error),
            Ok(_) => (),
        }
    }
    Ok(())
}

async fn create_subscription(
    config: &NatsSourceConfig,
) -> crate::Result<(async_nats::Connection, async_nats::Subscription)> {
    let nc = config.connect().await?;

    let subscription = match &config.queue {
        None => nc.subscribe(&config.subject).await,
        Some(queue) => nc.queue_subscribe(&config.subject, &queue).await,
    };

    let subscription = subscription?;

    Ok((nc, subscription))
}
