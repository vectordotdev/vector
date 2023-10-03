use vector_common::finalization::{EventFinalizers, Finalizable};
use vector_core::event::Event;

use crate::sinks::util::metadata::RequestMetadataBuilder;

pub struct EventMetadata<M = ()> {
    finalizers: EventFinalizers,
    metadata: M,
}

impl<M> EventMetadata<M> {
    pub fn from_metadata(finalizers: EventFinalizers, metadata: M) -> Self {
        Self {
            finalizers,
            metadata,
        }
    }

    pub fn into_parts(self) -> (EventFinalizers, M) {
        (self.finalizers, self.metadata)
    }
}

impl From<EventFinalizers> for EventMetadata<()> {
    fn from(finalizers: EventFinalizers) -> Self {
        Self {
            finalizers,
            metadata: (),
        }
    }
}

pub trait InputSplitter<Input> {
    type Metadata;
    type Output;

    fn split(
        input: Input,
    ) -> (
        EventMetadata<Self::Metadata>,
        RequestMetadataBuilder,
        Self::Output,
    );
}

#[derive(Default)]
pub struct GenericEventInputSplitter;

impl InputSplitter<Event> for GenericEventInputSplitter {
    type Metadata = ();
    type Output = Event;

    fn split(
        mut input: Event,
    ) -> (
        EventMetadata<Self::Metadata>,
        RequestMetadataBuilder,
        Self::Output,
    ) {
        let builder = RequestMetadataBuilder::from_event(&input);
        let finalizers = input.take_finalizers();
        (finalizers.into(), builder, input)
    }
}

impl InputSplitter<Vec<Event>> for GenericEventInputSplitter {
    type Metadata = ();
    type Output = Vec<Event>;

    fn split(
        mut input: Vec<Event>,
    ) -> (
        EventMetadata<Self::Metadata>,
        RequestMetadataBuilder,
        Self::Output,
    ) {
        let builder = RequestMetadataBuilder::from_events(input.as_ref());
        let finalizers = input.take_finalizers();
        (finalizers.into(), builder, input)
    }
}
