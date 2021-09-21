use crate::test::common::Message;
use proptest_derive::Arbitrary;

/// The action that our model interpreter loop will take.
#[derive(Debug, Clone, Arbitrary)]
pub enum Action {
    /// Send a [`Message`] through the buffer
    Send(Message),
    /// Receive a [`Message`] from the buffer
    Recv,
}
