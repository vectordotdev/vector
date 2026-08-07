use metrics::Counter;

use crate::counter;
use tracing::trace;

use super::{ByteSize, CounterName, Protocol, SharedString};

crate::registered_event!(
    BytesSent {
        protocol: SharedString,
        extra_labels: Vec<(SharedString, SharedString)>,
    } => {
        bytes_sent: Counter = {
            let mut labels: Vec<(String, String)> = vec![("protocol".to_string(), self.protocol.to_string())];
            for (k, v) in &self.extra_labels {
                labels.push((k.to_string(), v.to_string()));
            }
            counter!(CounterName::ComponentSentBytesTotal, &labels)
        },
        protocol: SharedString = self.protocol,
    }

    fn emit(&self, byte_size: ByteSize) {
        trace!(message = "Bytes sent.", byte_size = %byte_size.0, protocol = %self.protocol);
        self.bytes_sent.increment(byte_size.0 as u64);
    }
);

impl From<Protocol> for BytesSent {
    fn from(protocol: Protocol) -> Self {
        Self {
            protocol: protocol.0,
            extra_labels: vec![],
        }
    }
}
