use std::cell::UnsafeCell;

use super::AllocationGroup;

thread_local! {
    static ACCUMULATOR: UnsafeCell<Accumulator> = const { UnsafeCell::new(Accumulator::new()) };
}

pub struct Accumulator {
    group: Option<&'static AllocationGroup>,
    allocated_bytes: u64,
}

impl Accumulator {
    const fn new() -> Self {
        Self {
            group: None,
            allocated_bytes: 0,
        }
    }

    pub fn track_allocation_local(allocated_bytes: u64) {
        // SAFETY: The group stack is per-thread, so we are the only ones that could possibly be
        // accessing it here.
        unsafe {
            let _ = ACCUMULATOR.try_with(|acc| {
                let acc = &mut *acc.get();
                acc.allocated_bytes += allocated_bytes;
            });
        }
    }

    fn flush_and_reset(&mut self) {
        let allocated_bytes = self.allocated_bytes;
        self.allocated_bytes = 0;

        let group = self
            .group
            .expect("must be set if calling `flush_and_reset`");
        group.track_allocation(allocated_bytes);
    }

    fn maybe_flush_and_swap(new_group: Option<&'static AllocationGroup>) {
        // SAFETY: The group stack is per-thread, so we are the only ones that could possibly be
        // accessing it here.
        unsafe {
            let _ = ACCUMULATOR.try_with(|acc| {
                let acc = &mut *acc.get();
                match (acc.group, new_group) {
                    // We had no current group being accumulated, and have no group to accumulate
                    // for, so we do nothing.
                    //
                    // TODO: This should probably be a panic since it shouldn't be possible to exit
                    // an accumulator when no allocation group is being accumulated for.
                    (None, None) => {}
                    // Staring to accumulate for an allocation group and no other allocation group
                    // is being accumulated for, so just set the allocation group.
                    (None, new_group) => {
                        acc.group = new_group;
                    }
                    // We're currently accumulating for an allocation group, but are being cleared
                    // out, so just do a normal flush.
                    (Some(_), None) => {
                        acc.flush_and_reset();
                        acc.group = None;
                    }
                    // We're currently accumulating for an allocation group, but are being set to a
                    // new allocation group. If it's the same allocation group (i.e. reentrant) we
                    // continue accumulating, otherwise, we flush and set the new allocation group
                    // as our group to accumulate for.
                    (Some(group), Some(new_group)) => {
                        if !std::ptr::eq(group, new_group) {
                            acc.flush_and_reset();
                            acc.group = Some(new_group);
                        }
                    }
                }
            });
        }
    }

    pub fn enter(group: &'static AllocationGroup) {
        Self::maybe_flush_and_swap(Some(group));
    }

    pub fn exit() {
        Self::maybe_flush_and_swap(None);
    }
}
