use futures::{Future, Sink};

pub mod elasticsearch;
pub mod splunk;
mod util;

use crate::record::Record;

pub type RouterSink = Box<dyn Sink<SinkItem = Record, SinkError = ()> + 'static + Send>;
pub type RouterSinkFuture = Box<dyn Future<Item = RouterSink, Error = ()> + Send>;
