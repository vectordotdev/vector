use std::sync::atomic::AtomicU8;

/// Number of slots in the [`VECTOR_COMPONENT_LABELS`] array (4 KiB).
pub const LABELS_LEN: usize = 4096;

/// Shared-memory array indexed by `tid % LABELS_LEN`.
///
/// Rust writes the component's group ID on enter and 0 on exit (~1ns, no kernel
/// involvement).  bpftrace reads this array on a fixed-rate profile timer
/// (`profile:hz:997`) to attribute stack samples to components.
///
/// 4096 entries = 4KB.  TID collision requires two Vector threads whose TIDs
/// differ by exactly 4096 — effectively impossible for a process with ~8-32
/// threads.
#[unsafe(no_mangle)]
#[allow(clippy::declare_interior_mutable_const)]
pub static VECTOR_COMPONENT_LABELS: [AtomicU8; LABELS_LEN] = {
    const ZERO: AtomicU8 = AtomicU8::new(0);
    [ZERO; LABELS_LEN]
};

/// Uprobe attachment point: called once per component at startup to register
/// the mapping from allocation group ID to component name.
///
/// bpftrace attaches `uprobe:BINARY:vector_register_component` here at probe
/// startup to build a `group_id -> component_id` lookup table.  It also
/// captures `labels_ptr` / `labels_len` on the first call so the profile
/// handler can read the shared-memory array at runtime (ASLR makes the
/// compile-time address unusable).
///
/// Arguments follow the C ABI so bpftrace can read them reliably:
///   arg0 = group_id (u8)
///   arg1/arg2 = component_id (ptr, len)
///   arg3/arg4 = labels array (ptr, len)
///
/// `black_box` prevents LTO from eliding the call site.
#[unsafe(no_mangle)]
#[inline(never)]
#[allow(clippy::missing_const_for_fn)] // Must not be const: used as a uprobe attachment point.
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
        // Use a unique slot to avoid interference from other tests sharing the static array.
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
