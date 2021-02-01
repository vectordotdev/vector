use crate::{
    buffers::Acker,
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    emit,
    internal_events::{NatsEventMissingKeys, NatsEventSendFail, NatsEventSendSuccess},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        StreamSink,
    },
    template::{Template, TemplateError},
    Event,
};
use async_trait::async_trait;
use futures::{stream::BoxStream, FutureExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::convert::TryFrom;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("invalid subject template: {}", source))]
    SubjectTemplate { source: TemplateError },
}

/**
 * Code dealing with the SinkConfig struct.
 */

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NatsSinkConfig {
    encoding: EncodingConfig<Encoding>,
    #[serde(default = "default_name")]
    name: String,
    subject: String,
    url: String,
}

fn default_name() -> String {
    String::from("vector")
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

inventory::submit! {
    SinkDescription::new::<NatsSinkConfig>("nats")
}

impl GenerateConfig for NatsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            encoding.codec = "json"
            name = "vector"
            subject = "from.vector"
            url = "nats://127.0.0.1:4222""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "nats")]
impl SinkConfig for NatsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink = NatsSink::new(self.clone(), cx.acker())?;
        let healthcheck = healthcheck(self.clone()).boxed();
        Ok((super::VectorSink::Stream(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "nats"
    }
}

impl NatsSinkConfig {
    fn to_nats_options(&self) -> crate::Result<nats::Options> {
        // Set reconnect_buffer_size on the nats client to 0 bytes so that the
        // client doesn't buffer internally (to avoid message loss).
        let options = nats::Options::new()
            .with_name(&self.name)
            .reconnect_buffer_size(0);

        Ok(options)
    }

    async fn connect(&self) -> crate::Result<nats::asynk::Connection> {
        self.to_nats_options()?
            .connect_async(&self.url)
            .map_err(|e| e.into())
            .await
    }
}

async fn healthcheck(config: NatsSinkConfig) -> crate::Result<()> {
    config.connect().map_ok(|_| ()).await
}

/**
 * Code dealing with the Sink struct.
 */

#[derive(Clone)]
struct NatsOptions {
    name: String,
}

pub struct NatsSink {
    encoding: EncodingConfig<Encoding>,
    options: NatsOptions,
    subject: Template,
    url: String,
    acker: Acker,
}

impl NatsSink {
    fn new(config: NatsSinkConfig, acker: Acker) -> crate::Result<Self> {
        Ok(NatsSink {
            options: (&config).into(),
            encoding: config.encoding,
            subject: Template::try_from(config.subject).context(SubjectTemplate)?,
            url: config.url,
            acker,
        })
    }
}

impl From<NatsOptions> for nats::Options {
    fn from(options: NatsOptions) -> Self {
        nats::Options::new()
            .with_name(&options.name)
            .reconnect_buffer_size(0)
    }
}

impl From<&NatsSinkConfig> for NatsOptions {
    fn from(options: &NatsSinkConfig) -> Self {
        Self {
            name: options.name.clone(),
        }
    }
}

#[async_trait]
impl StreamSink for NatsSink {
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let nats_options: nats::Options = self.options.clone().into();

        let nc = nats_options
            .connect_async(&self.url)
            .await
            .map_err(|_| ())?;

        while let Some(event) = input.next().await {
            let subject = self.subject.render_string(&event).map_err(|missing_keys| {
                emit!(NatsEventMissingKeys {
                    keys: &missing_keys
                });
            })?;

            let log = encode_event(event, &self.encoding);
            let message_len = log.len();

            match nc.publish(&subject, log).await {
                Ok(_) => {
                    emit!(NatsEventSendSuccess {
                        byte_size: message_len,
                    });
                }
                Err(error) => {
                    emit!(NatsEventSendFail { error });
                }
            }

            self.acker.ack(1);
        }

        Ok(())
    }
}

fn encode_event(mut event: Event, encoding: &EncodingConfig<Encoding>) -> String {
    encoding.apply_rules(&mut event);

    match encoding.codec() {
        Encoding::Json => serde_json::to_string(event.as_log()).unwrap(),
        Encoding::Text => event
            .as_log()
            .get(crate::config::log_schema().message_key())
            .map(|v| v.to_string_lossy())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{encode_event, Encoding, EncodingConfig};
    use crate::event::{Event, Value};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<NatsSinkConfig>();
    }

    #[test]
    fn encodes_raw_logs() {
        let event = Event::from("foo");
        assert_eq!(
            "foo",
            encode_event(event, &EncodingConfig::from(Encoding::Text))
        );
    }

    #[test]
    fn encodes_log_events() {
        let mut event = Event::new_empty_log();
        let log = event.as_mut_log();
        log.insert("x", Value::from("23"));
        log.insert("z", Value::from(25));
        log.insert("a", Value::from("0"));

        let encoded = encode_event(event, &EncodingConfig::from(Encoding::Json));
        let expected = r#"{"a":"0","x":"23","z":25}"#;
        assert_eq!(encoded, expected);
    }
}

#[cfg(feature = "nats-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::test_util::{random_lines_with_stream, random_string, trace_init};
    use futures::stream::StreamExt;
    use std::{thread, time::Duration};

    #[tokio::test]
    async fn nats_happy() {
        // Publish `N` messages to NATS.
        //
        // Verify with a separate subscriber that the messages were
        // successfully published.

        trace_init();

        let subject = format!("test-{}", random_string(10));

        let cnf = NatsSinkConfig {
            encoding: EncodingConfig::from(Encoding::Text),
            name: "".to_owned(),
            subject: subject.clone(),
            url: "nats://127.0.0.1:4222".to_owned(),
        };

        // Establish the consumer subscription.
        let consumer = cnf.clone().connect().await.unwrap();
        let sub = consumer.subscribe(&subject).await.unwrap();

        // Publish events.
        let (acker, ack_counter) = Acker::new_for_testing();
        let mut sink = NatsSink::new(cnf.clone(), acker).unwrap();
        let num_events = 1_000;
        let (input, events) = random_lines_with_stream(100, num_events);

        let _ = sink.run(Box::pin(events)).await.unwrap();

        // Unsubscribe from the channel.
        thread::sleep(Duration::from_secs(3));
        let _ = sub.drain().await.unwrap();

        let output: Vec<String> = sub
            .map(|msg| String::from_utf8_lossy(&msg.data).to_string())
            .collect()
            .await;

        assert_eq!(output.len(), input.len());
        assert_eq!(output, input);

        assert_eq!(
            ack_counter.load(std::sync::atomic::Ordering::Relaxed),
            num_events
        );
    }
}
