use std::sync::atomic::AtomicU8;

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
pub extern "C" fn vector_register_component(
    id: u8,
    name_ptr: *const u8,
    name_len: usize,
    labels_ptr: *const u8,
    labels_len: usize,
) {
    std::hint::black_box((id, name_ptr, name_len, labels_ptr, labels_len));
}
