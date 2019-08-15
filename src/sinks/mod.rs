use futures::{Future, Sink};

pub mod aws_cloudwatch_logs;
pub mod aws_cloudwatch_metrics;
pub mod aws_kinesis_streams;
pub mod aws_s3;
pub mod blackhole;
pub mod console;
pub mod elasticsearch;
pub mod http;
#[cfg(feature = "rdkafka")]
pub mod kafka;
pub mod prometheus;
pub mod splunk_hec;
pub mod tcp;
pub mod util;
pub mod vector;

use crate::Event;

pub type RouterSink = Box<dyn Sink<SinkItem = Event, SinkError = ()> + 'static + Send>;

pub type Healthcheck = Box<dyn Future<Item = (), Error = String> + Send>;
