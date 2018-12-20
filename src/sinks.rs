use futures::{Future, Sink};

pub mod elasticsearch;
pub mod s3;
pub mod splunk;
pub mod util;

use crate::record::Record;

pub type RouterSink = Box<dyn Sink<SinkItem = Record, SinkError = ()> + 'static + Send>;
pub type RouterSinkFuture = Box<dyn Future<Item = RouterSink, Error = ()> + Send>;
