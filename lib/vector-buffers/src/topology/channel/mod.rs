mod limited_queue;
mod receiver;
mod sender;

pub use limited_queue::{LimitedReceiver, LimitedSender, SendError, limited};
pub use receiver::*;
pub use sender::*;

#[cfg(test)]
mod tests;
