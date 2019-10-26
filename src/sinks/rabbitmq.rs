use crate::{
  buffers::Acker,
  event::{self, Event},
  sinks::util::MetadataFuture,
  topology::config::{DataType, SinkConfig},
};
use futures::{
  future::{self, poll_fn},
  stream::FuturesUnordered,
  Async, AsyncSink, Future, Poll, Sink, StartSend, Stream,
};
use lapin_futures::{
  auth::SASLMechanism,
  options::{BasicPublishOptions, QueueDeclareOptions},
  types::FieldTable,
  BasicProperties, Client, ConfirmationFuture, ConnectionProperties,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum SASLMechanismDef {
  AMQPlain,
  External,
  Plain,
  RabbitCrDemo,
}

impl SASLMechanismDef {
  pub fn to_sasl_mechanism(&self) -> SASLMechanism {
    match &self {
      SASLMechanismDef::AMQPlain => SASLMechanism::AMQPlain,
      SASLMechanismDef::External => SASLMechanism::External,
      SASLMechanismDef::Plain => SASLMechanism::Plain,
      SASLMechanismDef::RabbitCrDemo => SASLMechanism::RabbitCrDemo,
    }
  }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConnectionPropertiesDef {
  pub mechanism: SASLMechanismDef,
  pub locale: String,
  pub client_properties: FieldTable,
  pub max_executor_threads: usize,
}

impl Default for ConnectionPropertiesDef {
  fn default() -> ConnectionPropertiesDef {
    ConnectionPropertiesDef {
      mechanism: SASLMechanismDef::Plain,
      locale: "en_US".into(),
      client_properties: FieldTable::default(),
      max_executor_threads: 1,
    }
  }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct QueueDeclareOptionsDef {
  pub passive: bool,
  pub durable: bool,
  pub exclusive: bool,
  pub auto_delete: bool,
  pub nowait: bool,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct BasicPublishOptionsDef {
  pub mandatory: bool,
  pub immediate: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RabbitMQSinkConfig {
  addr: String,
  basic_publish_options: BasicPublishOptionsDef,
  connection_properties: ConnectionPropertiesDef,
  encoding: Encoding,
  exchange: String,
  field_table: FieldTable,
  queue_name: String,
  queue_declare_options: QueueDeclareOptionsDef,
}

impl RabbitMQSinkConfig {
  pub fn connection_properties(&self) -> ConnectionProperties {
    ConnectionProperties {
      mechanism: self.connection_properties.mechanism.to_sasl_mechanism(),
      locale: self.connection_properties.locale.clone(),
      client_properties: self.connection_properties.client_properties.clone(),
      executor: None,
      max_executor_threads: self.connection_properties.max_executor_threads,
    }
  }

  pub fn queue_declare_options(&self) -> QueueDeclareOptions {
    QueueDeclareOptions {
      passive: self.queue_declare_options.passive,
      durable: self.queue_declare_options.durable,
      exclusive: self.queue_declare_options.exclusive,
      auto_delete: self.queue_declare_options.auto_delete,
      nowait: self.queue_declare_options.nowait,
    }
  }

  pub fn basic_publish_options(&self) -> BasicPublishOptions {
    BasicPublishOptions {
      immediate: self.basic_publish_options.mandatory,
      mandatory: self.basic_publish_options.mandatory,
    }
  }
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
  Text,
  Json,
}

pub struct RabbitMQSink {
  acker: Acker,
  basic_publish_options: BasicPublishOptions,
  channel: lapin_futures::Channel,
  encoding: Encoding,
  exchange: String,
  in_flight: FuturesUnordered<MetadataFuture<ConfirmationFuture<()>, ()>>,
  queue_name: String,
}

impl RabbitMQSink {
  fn new(config: RabbitMQSinkConfig, acker: Acker) -> crate::Result<Self> {
    let channel = Client::connect(&config.addr, config.connection_properties())
      .and_then(|client| client.create_channel())
      .wait()?;
    channel
      .queue_declare(
        &config.queue_name,
        config.queue_declare_options(),
        config.field_table.clone(),
      )
      .wait()?;
    Ok(RabbitMQSink {
      acker,
      basic_publish_options: config.basic_publish_options(),
      channel,
      encoding: config.encoding,
      exchange: config.exchange,
      in_flight: FuturesUnordered::new(),
      queue_name: config.queue_name,
    })
  }
}

#[typetag::serde(name = "rabbitmq")]
impl SinkConfig for RabbitMQSinkConfig {
  fn build(&self, acker: Acker) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
    let sink = RabbitMQSink::new(self.clone(), acker)?;
    let hc = healthcheck(self.clone());
    Ok((Box::new(sink), hc))
  }

  fn input_type(&self) -> DataType {
    DataType::Log
  }
}

impl Sink for RabbitMQSink {
  type SinkItem = Event;
  type SinkError = ();

  fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
    let payload = encode_event(&item, &self.encoding);
    let future = self.channel.basic_publish(
      &self.exchange,
      &self.queue_name,
      payload,
      self.basic_publish_options.clone(),
      BasicProperties::default(),
    );
    self.in_flight.push(future.join(future::ok(())));
    Ok(AsyncSink::Ready)
  }

  fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
    loop {
      match self.in_flight.poll() {
        // nothing ready yet
        Ok(Async::NotReady) => return Ok(Async::NotReady),

        // nothing in flight
        Ok(Async::Ready(None)) => return Ok(Async::Ready(())),

        // request finished, check for success
        Ok(Async::Ready(Some(((), _)))) => {
          trace!("published message to rabbitmq");
        }

        Err(e) => error!("publishing message failed: {}", e),
      }
    }
  }
}

fn healthcheck(config: RabbitMQSinkConfig) -> super::Healthcheck {
  let check = poll_fn(move || Ok(Async::Ready(())));

  Box::new(check)
}

fn encode_event(event: &Event, encoding: &Encoding) -> Vec<u8> {
  let payload = match encoding {
    &Encoding::Json => serde_json::to_vec(&event.as_log().clone().unflatten()).unwrap(),
    &Encoding::Text => event
      .as_log()
      .get(&event::MESSAGE)
      .map(|v| v.as_bytes().to_vec())
      .unwrap_or(Vec::new()),
  };

  payload
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn simple_test() {
    let config = RabbitMQSinkConfig {
      addr: String::from("amqp://127.0.0.1:5672/%2f"),
      basic_publish_options: BasicPublishOptionsDef::default(),
      connection_properties: ConnectionPropertiesDef::default(),
      encoding: Encoding::Text,
      exchange: String::from(""),
      field_table: FieldTable::default(),
      queue_name: String::from("hello"),
      queue_declare_options: QueueDeclareOptionsDef::default(),
    };
    let acker = Acker::Null;
    let rabbit = config.build(acker).unwrap();
  }
}
