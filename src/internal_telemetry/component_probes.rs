use std::sync::atomic::{AtomicU8, Ordering};

/// Returns a leaked `&'static AtomicU8` unique to the current thread.
///
/// On first access, allocates a single byte via `Box::leak`, then calls
/// [`vector_register_thread`] so bpftrace can record the mapping from
/// this thread's TID to the byte's address.
///
/// The leaked byte lives for the lifetime of the process — no use-after-free
/// is possible even if the thread exits.
pub fn thread_label() -> &'static AtomicU8 {
    thread_local! {
        static LABEL: &'static AtomicU8 = {
            let label: &'static AtomicU8 = Box::leak(Box::new(AtomicU8::new(0)));
            #[cfg(target_os = "linux")]
            {
                // SAFETY: gettid() is always safe on Linux.
                let tid = unsafe { libc::gettid() } as u64;
                vector_register_thread(tid, label as *const AtomicU8 as *const u8);
            }
            label
        };
    }
    LABEL.with(|l| *l)
}

/// Uprobe attachment point: called once per thread to register the mapping
/// from Linux TID to the address of that thread's label byte.
///
/// bpftrace attaches `uprobe:BINARY:vector_register_thread` to build a
/// `@tid_to_addr[tid] = addr` map. On each `profile:hz:997` sample it reads
/// `*((uint8 *)@tid_to_addr[tid])` to get the active component group ID.
///
/// Arguments follow the C ABI:
///   arg0 = tid (u64)
///   arg1 = label_ptr (*const u8)
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn vector_register_thread(tid: u64, label_ptr: *const u8) {
    std::hint::black_box((tid, label_ptr));
}

/// Uprobe attachment point: called once per component at startup to register
/// the mapping from allocation group ID to component name.
///
/// bpftrace attaches `uprobe:BINARY:vector_register_component` to build a
/// `@component_names[group_id] = name` lookup table.
///
/// Arguments follow the C ABI:
///   arg0 = group_id (u8)
///   arg1/arg2 = component_id (ptr, len)
#[unsafe(no_mangle)]
#[inline(never)]
#[allow(clippy::missing_const_for_fn)]
pub extern "C" fn vector_register_component(
    id: u8,
    name_ptr: *const u8,
    name_len: usize,
) {
    std::hint::black_box((id, name_ptr, name_len));
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    #[test]
    fn thread_label_store_and_clear() {
        let label = thread_label();
        let group_id: u8 = 7;

        label.store(group_id, Ordering::Relaxed);
        assert_eq!(label.load(Ordering::Relaxed), group_id);

        label.store(0, Ordering::Relaxed);
        assert_eq!(label.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn thread_label_is_stable() {
        let a = thread_label();
        let b = thread_label();
        assert!(std::ptr::eq(a, b), "must return the same address");
    }

    #[test]
    fn thread_labels_are_unique() {
        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel();
        for _ in 0..4 {
            let tx = tx.clone();
            std::thread::spawn(move || {
                tx.send(thread_label() as *const AtomicU8 as usize)
                    .unwrap();
            });
        }
        drop(tx);
        let mut addrs: Vec<usize> = rx.iter().collect();
        addrs.sort();
        addrs.dedup();
        assert_eq!(addrs.len(), 4, "each thread must get a distinct address");
    }

    #[test]
    fn register_component_does_not_panic() {
        let name = b"test_component";
        vector_register_component(1, name.as_ptr(), name.len());
    }
}
