use super::token::AllocationGroupId;

/// Traces allocations and deallocations.
pub trait Tracer {
    /// Traces an allocation.
    ///
    /// All allocations/deallocations that occur within the call to `Tracer::trace_allocation` are ignored, so
    /// implementors can allocate/deallocate without risk of reentrancy bugs. It does mean, however, that the
    /// allocations/deallocations that occur will be effectively lost, so implementors should ensure that the only data
    /// they deallocate in the tracer is data that was similarly allocated, and vice versa.
    ///
    /// The object size is from the original layout excluding the group ID size.
    fn trace_allocation(&self, object_size: usize, group_id: AllocationGroupId);

    /// Traces a deallocation.
    ///
    /// `source_group_id` contains the group ID where the given allocation originated from, while `current_group_id` is
    /// the current group ID, and as such, these values may differ depending on how values have had their ownership
    /// transferred.
    ///
    /// All allocations/deallocations that occur within the call to `Tracer::trace_deallocation` are ignored, so
    /// implementors can allocate/deallocate without risk of reentrancy bugs. It does mean, however, that the
    /// allocations/deallocations that occur will be effectively lost, so implementors should ensure that the only data
    /// they deallocate in the tracer is data that was similarly allocated, and vice versa.
    ///
    /// The object size is from the original layout excluding the group ID size.
    fn trace_deallocation(&self, object_size: usize, source_group_id: AllocationGroupId);
}
