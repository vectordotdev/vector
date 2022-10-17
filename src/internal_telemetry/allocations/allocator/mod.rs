use std::sync::atomic::{AtomicBool, Ordering};

mod stack;
mod token;
mod tracer;
mod tracing;
mod tracing_allocator;
mod util;

pub use self::token::AllocationGroupId;
pub use self::token::AllocationGroupToken;
pub use self::tracer::Tracer;
pub use self::tracing::AllocationLayer;
pub use self::tracing_allocator::GroupedTraceableAllocator;

/// Whether or not allocations and deallocations should be traced.
static TRACING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enables the tracing of allocations.
pub fn enable_allocation_tracing() {
    TRACING_ENABLED.store(true, Ordering::SeqCst);
}

/// Returns `true` if allocation tracing is enabled.
pub fn is_allocation_tracing_enabled() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed)
}
