use super::token::AllocationGroupId;

/// Traces allocations and deallocations.
pub trait Tracer {
    /// Traces an allocation.
    ///
    /// All allocations/deallocations that occur within the call to `Tracer::trace_allocation` are ignored, so
    /// implementors can allocate/deallocate without risk of reentrancy bugs. It does mean, however, that the
    /// allocations/deallocations that occur will be effectively lost, so implementors should ensure that the only data
    /// they deallocate in the tracer is data that was similarly allocated, and vise versa.
    ///
    /// As the allocator will customize the layout to include the group ID which owns an allocation, we provide two
    /// sizes: the object size and the wrapped size. The object size is the original layout of the allocation, and is
    /// valid against the given object address. The wrapped size is the true size of the underlying allocation that is
    /// made, and represents the actual memory usage for the given allocation.
    fn trace_allocation(&self, wrapped_size: usize, group_id: AllocationGroupId);

    /// Traces a deallocation.
    ///
    /// `source_group_id` contains the group ID where the given allocation originated from, while `current_group_id` is
    /// the current group ID, and as such, these values may differ depending on how values have had their ownership
    /// transferred.
    ///
    /// All allocations/deallocations that occur within the call to `Tracer::trace_deallocation` are ignored, so
    /// implementors can allocate/deallocate without risk of reentrancy bugs. It does mean, however, that the
    /// allocations/deallocations that occur will be effectively lost, so implementors should ensure that the only data
    /// they deallocate in the tracer is data that was similarly allocated, and vise versa.
    ///
    /// As the allocator will customize the layout to include the group ID which owns an allocation, we provide two
    /// sizes: the object size and the wrapped size. The object size is the original layout of the allocation, and is
    /// valid against the given object address. The wrapped size is the true size of the underlying allocation that is
    /// made, and represents the actual memory usage for the given allocation.
    fn trace_deallocation(&self, wrapped_size: usize, source_group_id: AllocationGroupId);
}
