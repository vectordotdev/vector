use std::sync::atomic::AtomicU8;

/// Number of slots in the [`VECTOR_COMPONENT_LABELS`] array.
///
/// Matches the default Linux `pid_max` (32768). TIDs are indexed via
/// `tid % LABELS_LEN`, so collisions are only possible when two threads in
/// the same process have TIDs that differ by an exact multiple of 32768 —
/// this cannot happen unless `pid_max` has been raised above the default,
/// which is uncommon. In that case the only consequence is mislabeled
/// profiling samples.
///
/// 32768 entries = 32 KiB of static memory.
pub const LABELS_LEN: usize = 32768;

/// Per-thread label array indexed by `tid % LABELS_LEN`.
///
/// On span enter, the component's allocation group ID is written; on span
/// exit it is cleared to 0.
///
/// bpftrace can read this array on a fixed-rate profile timer
/// (`profile:hz:997`) using `bpf_probe_read_user` to attribute CPU samples
/// to individual components.
#[unsafe(no_mangle)]
#[allow(clippy::declare_interior_mutable_const)]
pub static VECTOR_COMPONENT_LABELS: [AtomicU8; LABELS_LEN] = {
    const ZERO: AtomicU8 = AtomicU8::new(0);
    [ZERO; LABELS_LEN]
};

/// bpftrace attaches `uprobe:BINARY:vector_register_component` to build a
/// `@component_names[group_id] = name` lookup table. It also captures
/// `labels_ptr` and `labels_len` on the first call so the profile handler can
/// read the shared-memory array at runtime.
///
#[unsafe(no_mangle)]
#[inline(never)]
#[allow(clippy::missing_const_for_fn)]
pub extern "C" fn vector_register_component(
    id: u8,
    name_ptr: *const u8,
    name_len: usize,
    labels_ptr: *const u8,
    labels_len: usize,
) {
    std::hint::black_box((id, name_ptr, name_len, labels_ptr, labels_len));
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    #[test]
    fn labels_array_store_and_clear() {
        let slot = 99;
        let group_id: u8 = 7;

        VECTOR_COMPONENT_LABELS[slot].store(group_id, Ordering::Relaxed);
        assert_eq!(
            VECTOR_COMPONENT_LABELS[slot].load(Ordering::Relaxed),
            group_id
        );

        VECTOR_COMPONENT_LABELS[slot].store(0, Ordering::Relaxed);
        assert_eq!(VECTOR_COMPONENT_LABELS[slot].load(Ordering::Relaxed), 0);
    }

    #[test]
    fn register_component_does_not_panic() {
        let name = b"test_component";
        vector_register_component(
            1,
            name.as_ptr(),
            name.len(),
            VECTOR_COMPONENT_LABELS.as_ptr() as *const u8,
            LABELS_LEN,
        );
    }
}
