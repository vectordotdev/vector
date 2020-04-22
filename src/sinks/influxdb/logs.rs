use crate::{
    buffers::Acker,
    emit,
    event::{self, Event},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures01::{future, AsyncSink, Future, Poll, Sink, StartSend};
use serde::{Deserialize, Serialize};

pub struct InfluxDBLogsSink {
    total_events: usize,
    total_raw_bytes: usize,
    config: InfluxDBLogsConfig,
    acker: Acker,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct InfluxDBLogsConfig {
    pub print_amount: usize,
}

inventory::submit! {
    SinkDescription::new_without_default::<InfluxDBLogsConfig>("influxdb_logs")
}

#[typetag::serde(name = "influxdb_logs")]
impl SinkConfig for InfluxDBLogsConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = Box::new(InfluxDBLogsSink::new(self.clone(), cx.acker()));
        let healthcheck = Box::new(healthcheck());

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "influxdb_logs"
    }
}

fn healthcheck() -> impl Future<Item = (), Error = crate::Error> {
    future::ok(())
}

impl InfluxDBLogsSink {
    pub fn new(config: InfluxDBLogsConfig, acker: Acker) -> Self {
        InfluxDBLogsSink {
            config,
            total_events: 0,
            total_raw_bytes: 0,
            acker,
        }
    }
}

impl Sink for InfluxDBLogsSink {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let message_len = match item {
            Event::Log(log) => log
                .get(&event::log_schema().message_key())
                .map(|v| v.as_bytes().len())
                .unwrap_or(0),
            Event::Metric(metric) => serde_json::to_string(&metric).map(|v| v.len()).unwrap_or(0),
        };

        self.total_events += 1;
        self.total_raw_bytes += message_len;

        if self.total_events % self.config.print_amount == 0 {
            info!({
                events = self.total_events,
                raw_bytes_collected = self.total_raw_bytes
            }, "Total events collected");
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
    use crate::test_util::random_events_with_stream;

    #[test]
    fn influxdb_logs() {
        let config = InfluxDBLogsConfig { print_amount: 10 };
        let sink = InfluxDBLogsSink::new(config, Acker::Null);

        let (_input_lines, events) = random_events_with_stream(100, 10);

        let _ = sink.send_all(events).wait().unwrap();
    }
}
