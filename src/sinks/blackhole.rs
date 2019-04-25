use crate::buffers::Acker;
use crate::record::{self, Record};
use futures::{future, AsyncSink, Future, Poll, Sink, StartSend};
use serde::{Deserialize, Serialize};

pub struct BlackholeSink {
    total_records: usize,
    total_raw_bytes: usize,
    config: BlackholeConfig,
    acker: Acker,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BlackholeConfig {
    print_amount: usize,
}

#[typetag::serde(name = "blackhole")]
impl crate::topology::config::SinkConfig for BlackholeConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let sink = Box::new(BlackholeSink::new(self.clone(), acker));
        let healthcheck = Box::new(healthcheck());

        Ok((sink, healthcheck))
    }
}

fn healthcheck() -> impl Future<Item = (), Error = String> {
    future::ok(())
}

impl BlackholeSink {
    pub fn new(config: BlackholeConfig, acker: Acker) -> Self {
        BlackholeSink {
            config,
            total_records: 0,
            total_raw_bytes: 0,
            acker,
        }
    }
}

impl Sink for BlackholeSink {
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let message_len = item.structured[&record::MESSAGE].as_bytes().len();

        self.total_records += 1;
        self.total_raw_bytes += message_len;

        trace!(raw_bytes_counter = message_len, records_counter = 1);

        if self.total_records % self.config.print_amount == 0 {
            info!({
                records = self.total_records,
                raw_bytes_collected = self.total_raw_bytes
            }, "Total records collected");
        }

        self.acker.ack(1);

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffers::Acker;
    use crate::test_util::random_records_with_stream;
    use crate::topology::config::SinkConfig;

    #[test]
    fn blackhole() {
        let config = BlackholeConfig { print_amount: 10 };
        let (sink, _) = config.build(Acker::Null).unwrap();

        let (_input_lines, records) = random_records_with_stream(100, 10);

        sink.send_all(records).wait().unwrap();
    }
}
