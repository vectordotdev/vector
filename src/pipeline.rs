use crate::{transforms::FunctionTransform, Event};
use futures01::{
    sync::mpsc::{channel, Receiver, SendError, Sender},
    Async, AsyncSink, Poll, Sink,
};
use std::collections::VecDeque;

#[derive(Derivative, Clone)]
#[derivative(Debug)]
pub struct Pipeline {
    inner: Sender<Event>,
    // We really just keep this around in case we need to rebuild.
    #[derivative(Debug = "ignore")]
    inlines: Vec<Box<dyn FunctionTransform>>,
    enqueued: VecDeque<Event>,
}

impl Pipeline {
    fn try_flush(&mut self) -> Poll<(), <Self as Sink>::SinkError> {
        while let Some(event) = self.enqueued.pop_front() {
            if let AsyncSink::NotReady(item) = self.inner.start_send(event)? {
                self.enqueued.push_front(item);
                return Ok(Async::NotReady);
            }
        }
        Ok(Async::Ready(()))
    }
}

impl Sink for Pipeline {
    type SinkItem = Event;
    type SinkError = SendError<Self::SinkItem>;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        match self.try_flush() {
            Ok(Async::NotReady) => Ok(AsyncSink::NotReady(item)),
            Ok(Async::Ready(())) => {
                // Note how this gets **swapped** with `new_working_set` in the loop.
                // At the end of the loop, it will only contain finalized events.
                let mut working_set = vec![item];
                for inline in self.inlines.iter_mut() {
                    let mut new_working_set = Vec::with_capacity(working_set.len());
                    for event in working_set.drain(..) {
                        inline.transform(&mut new_working_set, event);
                    }
                    core::mem::swap(&mut new_working_set, &mut working_set);
                }
                self.enqueued.extend(working_set);
                Ok(AsyncSink::Ready)
            }
            Err(e) => Err(e),
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        futures01::try_ready!(self.try_flush());
        debug_assert!(self.enqueued.is_empty());
        self.inner.poll_complete()
    }
}

impl Pipeline {
    #[cfg(test)]
    pub fn new_test() -> (Self, Receiver<Event>) {
        Self::new_with_buffer(100, vec![])
    }

    pub fn new_with_buffer(
        n: usize,
        inlines: Vec<Box<dyn FunctionTransform>>,
    ) -> (Self, Receiver<Event>) {
        let (tx, rx) = channel(n);
        (Self::from_sender(tx, inlines), rx)
    }

    pub fn from_sender(inner: Sender<Event>, inlines: Vec<Box<dyn FunctionTransform>>) -> Self {
        Self {
            inner,
            inlines,
            // We ensure the buffer is sufficient that it is unlikely to require reallocations.
            // There is a possibility a component might blow this queue size.
            enqueued: VecDeque::with_capacity(10),
        }
    }

    pub fn poll_ready(&mut self) -> Poll<(), SendError<()>> {
        self.inner.poll_ready()
    }
}

#[cfg(all(test, feature = "transforms-add_fields", feature = "transforms-filter"))]
mod test {
    use super::*;
    use crate::{
        transforms::{add_fields::AddFields, filter::Filter},
        Event, Value,
    };
    use futures::compat::Future01CompatExt;
    use futures01::Stream;
    use serde_json::json;
    use std::convert::TryFrom;

    const KEYS: [&str; 2] = ["booper", "swooper"];

    const VALS: [&str; 2] = ["Pineapple", "Coconut"];

    #[tokio::test]
    async fn multiple_transforms() -> Result<(), crate::Error> {
        let transform_1 = AddFields::new(
            indexmap::indexmap! {
                KEYS[0].into() => Value::from(VALS[0]),
            },
            false,
        )?;
        let transform_2 = AddFields::new(
            indexmap::indexmap! {
                KEYS[1].into() => Value::from(VALS[1]),
            },
            false,
        )?;

        let (pipeline, reciever) =
            Pipeline::new_with_buffer(100, vec![Box::new(transform_1), Box::new(transform_2)]);

        let event = Event::try_from(json!({
            "message": "MESSAGE_MARKER",
        }))?;

        pipeline.send(event).compat().await?;
        let out = reciever.wait().collect::<Result<Vec<_>, ()>>().unwrap();

        assert_eq!(out[0].as_log().get(KEYS[0]), Some(&Value::from(VALS[0])));
        assert_eq!(out[0].as_log().get(KEYS[1]), Some(&Value::from(VALS[1])));

        Ok(())
    }

    #[tokio::test]
    async fn filtered_output() -> Result<(), crate::Error> {
        let transform_1 = Filter::new(Box::new(crate::conditions::check_fields::CheckFields::new(
            indexmap::indexmap! {
                KEYS[1].into() => crate::conditions::check_fields::EqualsPredicate::new(
                    "message".into(),
                    &crate::conditions::check_fields::CheckFieldsPredicateArg::String("NOT".into()),
                )?,
            },
        )));

        let (pipeline, reciever) = Pipeline::new_with_buffer(100, vec![Box::new(transform_1)]);

        let event = Event::try_from(json!({
            "message": "MESSAGE_MARKER",
        }))?;

        pipeline.send(event).compat().await?;
        let out = reciever.wait().collect::<Result<Vec<_>, ()>>().unwrap();

        assert_eq!(out, vec![]);

        Ok(())
    }
}
