#![allow(missing_docs)]

#[cfg(feature = "allocation-tracing")]
pub mod allocations;

pub const fn is_allocation_tracking_enabled() -> bool {
    cfg!(feature = "allocation-tracing")
}
