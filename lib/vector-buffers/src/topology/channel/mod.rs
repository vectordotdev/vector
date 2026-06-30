mod limited_queue;
mod receiver;
mod sender;

pub use limited_queue::{
    BufferChannelKind, ChannelMetricMetadata, DEFAULT_EWMA_HALF_LIFE_SECONDS, LimitedReceiver,
    LimitedSender, SendError, limited, limited_with_usage_handle,
};
pub use receiver::*;
pub use sender::*;

#[cfg(test)]
mod observer_tests;
#[cfg(test)]
mod tests;
