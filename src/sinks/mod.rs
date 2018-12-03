use futures::{Future, Sink};
use std::io;

pub mod elasticsearch;
pub mod splunk;

use Record;

pub type RouterSink = Box<dyn Sink<SinkItem = Record, SinkError = io::Error> + 'static + Send>;
pub type RouterSinkFuture = Box<dyn Future<Item = RouterSink, Error = io::Error> + Send>;

pub trait SinkFactory {
    type Config;

    fn build(config: Self::Config) -> RouterSinkFuture;
}
