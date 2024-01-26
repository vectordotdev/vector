use std::{any::TypeId, marker::PhantomData, ptr::addr_of};

use tracing::{Dispatch, Id, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

use super::AllocationGroupToken;

pub(crate) struct WithAllocationGroup {
    pub with_allocation_group: fn(&Dispatch, &Id, AllocationGroupToken),
}

/// [`AllocationLayer`] is a [`tracing_subscriber::Layer`] that handles entering and exiting an allocation
/// group as the span it is attached to is itself entered and exited.
///
/// More information on using this layer can be found in the examples, or directly in the
/// `tracing_subscriber` docs, found [here][tracing_subscriber::layer].
#[cfg_attr(docsrs, doc(cfg(feature = "tracing-compat")))]
pub struct AllocationLayer<S> {
    ctx: WithAllocationGroup,
    _subscriber: PhantomData<S>,
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
            ctx,
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
            if let Some(token) = span_ref.extensions().get::<AllocationGroupToken>() {
                token.enter();
            }
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span_ref) = ctx.span(id) {
            if let Some(token) = span_ref.extensions().get::<AllocationGroupToken>() {
                token.exit();
            }
        }
    }

    unsafe fn downcast_raw(&self, id: TypeId) -> Option<*const ()> {
        match id {
            id if id == TypeId::of::<Self>() => Some(addr_of!(self).cast::<()>()),
            id if id == TypeId::of::<WithAllocationGroup>() => {
                Some(addr_of!(self.ctx).cast::<()>())
            }
            _ => None,
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
