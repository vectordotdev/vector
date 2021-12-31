use bytes::Bytes;
use chrono::Utc;
use futures::{pin_mut, stream, SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use tokio_util::codec::FramedRead;

use crate::{
    codecs::{
        self,
        decoding::{DecodingConfig, DeserializerConfig, FramingConfig},
    },
    config::{
        log_schema, DataType, GenerateConfig, SourceConfig, SourceContext, SourceDescription,
    },
    event::Event,
    internal_events::NatsEventsReceived,
    serde::{default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    sources::util::StreamDecodingError,
    Pipeline,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Could not create Nats subscriber: {}", source))]
    NatsCreateError { source: std::io::Error },
    #[snafu(display("Could not subscribe to Nats topics: {}", source))]
    NatsSubscribeError { source: std::io::Error },
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct NatsSourceConfig {
    url: String,
    #[serde(alias = "name")]
    connection_name: String,
    subject: String,
    queue: Option<String>,
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    framing: Box<dyn FramingConfig>,
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    decoding: Box<dyn DeserializerConfig>,
}

inventory::submit! {
    SourceDescription::new::<NatsSourceConfig>("nats")
}

impl GenerateConfig for NatsSourceConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            connection_name = "vector"
            subject = "from.vector"
            url = "nats://127.0.0.1:4222""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "nats")]
impl SourceConfig for NatsSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let (connection, subscription) = create_subscription(self).await?;
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build()?;

        Ok(Box::pin(nats_source(
            connection,
            subscription,
            decoder,
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
            .with_name(&self.connection_name)
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
            .with_name(&config.connection_name)
            .reconnect_buffer_size(0)
    }
}

fn get_subscription_stream(
    subscription: async_nats::Subscription,
) -> impl Stream<Item = async_nats::Message> {
    stream::unfold(subscription, |subscription| async move {
        subscription.next().await.map(|msg| (msg, subscription))
    })
}

async fn nats_source(
    // Take ownership of the connection so it doesn't get dropped.
    _connection: async_nats::Connection,
    subscription: async_nats::Subscription,
    decoder: codecs::Decoder,
    shutdown: ShutdownSignal,
    mut out: Pipeline,
) -> Result<(), ()> {
    let stream = get_subscription_stream(subscription).take_until(shutdown);
    pin_mut!(stream);
    while let Some(msg) = stream.next().await {
        let mut stream = FramedRead::new(msg.data.as_ref(), decoder.clone());
        while let Some(next) = stream.next().await {
            match next {
                Ok((events, byte_size)) => {
                    emit!(&NatsEventsReceived {
                        byte_size,
                        count: events.len()
                    });

                    let now = Utc::now();

                    for mut event in events {
                        if let Event::Log(ref mut log) = event {
                            log.try_insert(log_schema().source_type_key(), Bytes::from("nats"));
                            log.try_insert(log_schema().timestamp_key(), now);
                        }

                        out.send(event)
                            .await
                            .map_err(|error: crate::pipeline::ClosedError| {
                                error!(message = "Error sending to sink.", %error);
                            })?;
                    }
                }
                Err(error) => {
                    // Error is logged by `crate::codecs::Decoder`, no further
                    // handling is needed here.
                    if !error.can_continue() {
                        break;
                    }
                }
            }
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
        Some(queue) => nc.queue_subscribe(&config.subject, queue).await,
    };

    let subscription = subscription?;

    Ok((nc, subscription))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::print_stdout)] //tests

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<NatsSourceConfig>();
    }
}

#[cfg(feature = "nats-integration-tests")]
#[cfg(test)]
mod integration_tests {
    #![allow(clippy::print_stdout)] //tests

    use super::*;
    use crate::test_util::{collect_n, random_string};

    #[tokio::test]
    async fn nats_happy() {
        let subject = format!("test-{}", random_string(10));

        let conf = NatsSourceConfig {
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4222".to_owned(),
            queue: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
        };

        let (nc, sub) = create_subscription(&conf).await.unwrap();
        let nc_pub = nc.clone();

        let (tx, rx) = Pipeline::new_test();
        let decoder = DecodingConfig::new(conf.framing.clone(), conf.decoding.clone())
            .build()
            .unwrap();
        tokio::spawn(nats_source(nc, sub, decoder, ShutdownSignal::noop(), tx));
        let msg = "my message";
        nc_pub.publish(&subject, msg).await.unwrap();

        let events = collect_n(rx, 1).await;
        println!("Received event  {:?}", events[0].as_log());
        assert_eq!(events[0].as_log()[log_schema().message_key()], msg.into());
    }
}
