mod limited_queue;
mod receiver;
mod sender;

pub use limited_queue::{
    ChannelMetricMetadata, LimitedReceiver, LimitedSender, SendError, limited,
};
pub use receiver::*;
pub use sender::*;
pub use vector_common::stats::DEFAULT_EWMA_ALPHA;

#[cfg(test)]
mod tests;
