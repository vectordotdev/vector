use std::marker::PhantomData;

use tracing::{Dispatch, Id, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

use super::AllocationGroupToken;

pub(crate) struct WithAllocationGroup {
    with_allocation_group: fn(&Dispatch, &Id, AllocationGroupToken),
}

impl WithAllocationGroup {
    pub fn with_allocation_group(
        &self,
        dispatch: &Dispatch,
        id: &Id,
        unsafe_token: AllocationGroupToken,
    ) {
        (self.with_allocation_group)(dispatch, id, unsafe_token);
    }
}

/// [`AllocationLayer`] is a [`tracing_subscriber::Layer`] that handles entering and exiting an allocation
/// group as the span it is attached to is itself entered and exited.
///
/// More information on using this layer can be found in the examples, or directly in the
/// `tracing_subscriber` docs, found [here][tracing_subscriber::layer].
#[cfg_attr(docsrs, doc(cfg(feature = "tracing-compat")))]
pub struct AllocationLayer<S> {
    _ctx: WithAllocationGroup,
    _subscriber: PhantomData<fn(S)>,
}

impl<S> AllocationLayer<S>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    /// Creates a new [`AllocationLayer`].
    #[must_use]
    pub fn new() -> Self {
        let ctx = WithAllocationGroup {
            with_allocation_group: Self::with_allocation_group,
        };

        Self {
            _ctx: ctx,
            _subscriber: PhantomData,
        }
    }

    fn with_allocation_group(dispatch: &Dispatch, id: &Id, unsafe_token: AllocationGroupToken) {
        let subscriber = dispatch
            .downcast_ref::<S>()
            .expect("subscriber should downcast to expected type; this is a bug!");
        let span = subscriber
            .span(id)
            .expect("registry should have a span for the current ID");

        span.extensions_mut().insert(unsafe_token);
    }
}

impl<S> Layer<S> for AllocationLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span_ref) = ctx.span(id) {
            if let Some(token) = span_ref.extensions_mut().get_mut::<AllocationGroupToken>() {
                token.enter();
            }
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span_ref) = ctx.span(id) {
            if let Some(token) = span_ref.extensions_mut().get_mut::<AllocationGroupToken>() {
                token.exit();
            }
        }
    }
}

impl<S> Default for AllocationLayer<S>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn default() -> Self {
        AllocationLayer::new()
    }
}
