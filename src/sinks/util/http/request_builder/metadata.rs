use vector_common::{
    byte_size_of::ByteSizeOf,
    finalization::{EventFinalizers, Finalizable},
    request_metadata::GetEventCountTags,
};
use vector_core::{event::Event, EstimatedJsonEncodedSizeOf};

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

impl<E> InputSplitter<E> for GenericEventInputSplitter
where
    E: ByteSizeOf + GetEventCountTags + EstimatedJsonEncodedSizeOf + Finalizable,
{
    type Metadata = ();
    type Output = E;

    fn split(mut input: E) -> (EventMetadata, RequestMetadataBuilder, Event) {
        let builder = RequestMetadataBuilder::from_event(&input);
        let finalizers = input.take_finalizers();
        (finalizers.into(), builder, input)
    }
}

impl<I, E> InputSplitter<I> for GenericEventInputSplitter
where
    I: AsRef<[E]> + Finalizable,
    E: ByteSizeOf + GetEventCountTags + EstimatedJsonEncodedSizeOf,
{
    type Metadata = ();
    type Output = I;

    fn split(mut input: I) -> (EventMetadata, RequestMetadataBuilder, Event) {
        let builder = RequestMetadataBuilder::from_events(input.as_ref());
        let finalizers = input.take_finalizers();
        (finalizers.into(), builder, input)
    }
}
