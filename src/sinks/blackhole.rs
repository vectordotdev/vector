use crate::record::Record;
use futures::{future, AsyncSink, Future, Poll, Sink, StartSend};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct BlackholeSink {
    total_records: usize,
    total_raw_bytes: usize,
    config: BlackholeConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BlackholeConfig {
    print_amount: usize,
}

#[typetag::serde(name = "blackhole")]
impl crate::topology::config::SinkConfig for BlackholeConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let sink = Box::new(BlackholeSink::new(self.clone()));
        let healthcheck = Box::new(healthcheck());

        Ok((sink, healthcheck))
    }
}

fn healthcheck() -> impl Future<Item = (), Error = String> {
    future::ok(())
}

impl BlackholeSink {
    pub fn new(config: BlackholeConfig) -> Self {
        BlackholeSink {
            config,
            total_records: 0,
            total_raw_bytes: 0,
        }
    }
}

impl Sink for BlackholeSink {
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.total_records += 1;
        self.total_raw_bytes += item.raw.len();

        trace!(raw_bytes_counter = item.raw.len(), records_counter = 1);

        if self.total_records % self.config.print_amount == 0 {
            info!({
                records = self.total_records,
                raw_bytes_collected = self.total_raw_bytes
            }, "Total records collected");
        }

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
        let config = BlackholeConfig { print_amount: 10 };
        let (sink, _) = config.build().unwrap();

        let (_input_lines, records) = random_records_with_stream(100, 10);

        sink.send_all(records).wait().unwrap();
    }
}
