use metrics::Counter;

use crate::counter;
use tracing::trace;

use super::{ByteSize, MetricName, Protocol, SharedString};

crate::registered_event!(
    BytesSent {
        protocol: SharedString,
    } => {
        bytes_sent: Counter = counter!(MetricName::ComponentSentBytesTotal, "protocol" => self.protocol.clone()),
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
        }
    }
}
