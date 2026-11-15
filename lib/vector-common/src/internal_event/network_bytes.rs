use metrics::Counter;

use super::{ByteSize, CounterName, SharedString};
use crate::counter;

// Register at the component boundary, rather than on an HTTP connection's background task, so
// subsequent emissions retain the component's metric labels.
crate::registered_event!(
    SinkNetworkBytesSent {
        protocol: SharedString,
    } => {
        bytes: Counter = counter!(CounterName::ComponentSentNetworkBytesTotal, "protocol" => self.protocol.clone()),
        protocol: SharedString = self.protocol,
    }

    fn emit(&self, byte_size: ByteSize) {
        self.bytes.increment(byte_size.0 as u64);
        trace!(message = "Network bytes sent.", byte_size = %byte_size.0, protocol = %self.protocol);
    }
);

crate::registered_event!(
    SourceNetworkBytesReceived {
        protocol: SharedString,
    } => {
        bytes: Counter = counter!(CounterName::ComponentReceivedNetworkBytesTotal, "protocol" => self.protocol.clone()),
        protocol: SharedString = self.protocol,
    }

    fn emit(&self, byte_size: ByteSize) {
        self.bytes.increment(byte_size.0 as u64);
        trace!(message = "Network bytes received.", byte_size = %byte_size.0, protocol = %self.protocol);
    }
);
