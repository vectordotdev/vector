use crate::record::Record;
use futures::{future, AsyncSink, Future, Poll, Sink, StartSend};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct BlackholeSink;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BlackholeConfig;

#[typetag::serde(name = "blackhole")]
impl crate::topology::config::SinkConfig for BlackholeConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let sink = Box::new(BlackholeSink);
        let healthcheck = Box::new(healthcheck());

        Ok((sink, healthcheck))
    }
}

fn healthcheck() -> impl Future<Item = (), Error = String> {
    future::ok(())
}

impl Sink for BlackholeSink {
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(&mut self, _item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::random_records_with_stream;
    use crate::topology::config::SinkConfig;

    #[test]
    fn blackhole() {
        let (sink, _) = BlackholeConfig.build().unwrap();

        let (_input_lines, records) = random_records_with_stream(100, 10);

        sink.send_all(records).wait().unwrap();
    }
}
