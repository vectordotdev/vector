use crate::{
  buffers::Acker,
  event::{self, Event},
  topology::config::{DataType, SinkConfig},
  Error,
};
use futures::{
  future::{self, poll_fn, IntoFuture},
  stream::FuturesUnordered,
  Async, AsyncSink, Future, Poll, Sink, StartSend, Stream,
};
use lapin::options::{BasicPublishOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Client, ConnectionProperties};
use lapin_futures as lapin;
use log::info;
use serde::{Deserialize, Serialize};
use std::{thread, time::Duration};
use string_cache::DefaultAtom as Atom;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RabbitMQSinkConfig {
  addr: String,
  encoding: Encoding,
  queue_name: String,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
  Text,
  Json,
}

pub struct RabbitMQSink {
  acker: Acker,
  channel: lapin_futures::Channel,
  encoding: Encoding,
  queue_name: String,
}

impl RabbitMQSink {
  fn new(
    config: RabbitMQSinkConfig,
    channel: lapin_futures::Channel,
    acker: Acker,
  ) -> crate::Result<Self> {
    Ok(RabbitMQSink {
      acker,
      channel,
      encoding: config.encoding,
      queue_name: config.queue_name,
    })
  }
}

#[typetag::serde(name = "rabbitmq")]
impl SinkConfig for RabbitMQSinkConfig {
  fn build(&self, acker: Acker) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
    let channel = Client::connect(&self.addr, ConnectionProperties::default())
      .and_then(|client| client.create_channel())
      .wait()
      .unwrap();
    channel
      .queue_declare(
        &self.queue_name,
        QueueDeclareOptions::default(),
        FieldTable::default(),
      )
      .wait()
      .unwrap();
    let sink = RabbitMQSink::new(self.clone(), channel.clone(), acker)?;
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
    self
      .channel
      .basic_publish(
        "",
        "hello",
        b"hello from tokio".to_vec(),
        BasicPublishOptions::default(),
        BasicProperties::default(),
      )
      .wait()
      .unwrap();
    Ok(AsyncSink::Ready)
  }

  fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
    Ok(Async::Ready(()))
  }
}

fn healthcheck(config: RabbitMQSinkConfig) -> super::Healthcheck {
  let check = poll_fn(move || Ok(Async::Ready(())));

  Box::new(check)
}

fn encode_event(
  event: &Event,
  key_field: &Option<Atom>,
  encoding: &Encoding,
) -> (Vec<u8>, Vec<u8>) {
  let key = key_field
    .as_ref()
    .and_then(|f| event.as_log().get(f))
    .map(|v| v.as_bytes().to_vec())
    .unwrap_or(Vec::new());

  let body = match encoding {
    &Encoding::Json => serde_json::to_vec(&event.as_log().clone().unflatten()).unwrap(),
    &Encoding::Text => event
      .as_log()
      .get(&event::MESSAGE)
      .map(|v| v.as_bytes().to_vec())
      .unwrap_or(Vec::new()),
  };

  (key, body)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn simple_test() {
    let config = RabbitMQSinkConfig {
      addr: String::from("amqp://127.0.0.1:5672/%2f"),
      encoding: Encoding::Text,
      queue_name: String::from("hello"),
    };
    let acker = Acker::Null;
    let mut rabbit = config.build(acker).unwrap();
  }
}
