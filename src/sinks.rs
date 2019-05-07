use futures::{Future, Sink};

pub mod blackhole;
pub mod cloudwatch_logs;
pub mod console;
pub mod elasticsearch;
pub mod encoders;
pub mod http;
#[cfg(feature = "rdkafka")]
pub mod kafka;
pub mod kinesis;
pub mod prometheus;
pub mod s3;
pub mod splunk;
pub mod tcp;
pub mod util;

use crate::Event;

pub type RouterSink = Box<dyn Sink<SinkItem = Event, SinkError = ()> + 'static + Send>;

pub type Healthcheck = Box<dyn Future<Item = (), Error = String> + Send>;
