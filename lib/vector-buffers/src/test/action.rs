use quickcheck::{single_shrinker, Arbitrary, Gen};

use crate::test::Message;

/// The action that our model interpreter loop will take.
#[derive(Debug, Clone)]
pub enum Action {
    /// Send a [`Message`] through the buffer
    Send(Message),
    /// Receive a [`Message`] from the buffer
    Recv,
}

impl Arbitrary for Action {
    fn arbitrary(g: &mut Gen) -> Self {
        if bool::arbitrary(g) {
            Action::Send(Message::arbitrary(g))
        } else {
            Action::Recv
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            Action::Send(val) => Box::new(val.shrink().map(Action::Send)),
            Action::Recv => single_shrinker(Action::Recv),
        }
    }
}
