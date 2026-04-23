//! Lightweight bpftrace-based per-component CPU attribution.
//!
//! When the `component-probes` feature is enabled, this module provides a
//! [`ComponentProbesLayer`] that tags each Tokio worker thread with the ID of
//! the currently executing component. External bpftrace scripts read this tag
//! on a profile timer to produce per-component flamegraphs.
//!
//! Two complementary mechanisms are exposed for bpftrace scripts:
//!
//! 1. **Per-thread `AtomicU32` + `vector_register_thread`** — a polling approach
//!    where the profiler reads user-space memory at a fixed sampling rate. Very
//!    low overhead when profiling, but requires `bpf_probe_read_user` support
//!    (works on x86_64 via legacy `bpf_probe_read`; fails on arm64).
//!
//! 2. **`vector_component_enter` / `vector_component_exit` uprobes** — an
//!    event-driven approach where bpftrace maintains the active component
//!    mapping in a BPF map. Works on all architectures and bpftrace versions.
//!    Slightly higher overhead when bpftrace is attached (~1–5 µs per span
//!    transition due to uprobe traps), but negligible when not profiling.

use std::{
    collections::HashMap,
    marker::PhantomData,
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
};

use tracing::{
    Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id},
};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

/// Returns a leaked `&'static AtomicU32` unique to the current thread.
///
/// On first access, allocates an `AtomicU32` via `Box::leak` and calls
/// [`vector_register_thread`] so bpftrace can map this thread's TID
/// to its address. The leaked allocation is valid for the process lifetime.
fn thread_label() -> &'static AtomicU32 {
    thread_local! {
        static LABEL: &'static AtomicU32 = {
            let label: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(0)));
            #[cfg(target_os = "linux")]
            {
                let tid = nix::unistd::gettid().as_raw() as u64;
                vector_register_thread(tid, label as *const AtomicU32 as *const u8);
            }
            label
        };
    }
    LABEL.with(|l| *l)
}

/// Uprobe attachment point called once per thread to register the
/// `tid -> label_address` mapping with bpftrace.
#[unsafe(no_mangle)]
#[inline(never)]
#[allow(clippy::missing_const_for_fn)]
pub extern "C" fn vector_register_thread(tid: u64, label_ptr: *const u8) {
    std::hint::black_box((tid, label_ptr));
}

/// Uprobe attachment point called once per component to register the
/// `group_id -> component_name` mapping with bpftrace.
#[unsafe(no_mangle)]
#[inline(never)]
#[allow(clippy::missing_const_for_fn)]
pub extern "C" fn vector_register_component(id: u32, name_ptr: *const u8, name_len: usize) {
    std::hint::black_box((id, name_ptr, name_len));
}

/// Uprobe attachment point fired on every component span enter.
///
/// bpftrace hooks this to record `tid -> component_id` in a BPF map,
/// avoiding user-space memory reads from `profile` probes (required
/// for arm64 compatibility where `bpf_probe_read` is kernel-only).
#[unsafe(no_mangle)]
#[inline(never)]
#[allow(clippy::missing_const_for_fn)]
pub extern "C" fn vector_component_enter(component_id: u32) {
    std::hint::black_box(component_id);
}

/// Uprobe attachment point fired on every component span exit.
///
/// bpftrace hooks this to clear the active component for the current
/// thread, setting the BPF map entry to 0 (idle).
#[unsafe(no_mangle)]
#[inline(never)]
#[allow(clippy::missing_const_for_fn)]
pub extern "C" fn vector_component_exit() {
    std::hint::black_box(());
}

/// Next probe group ID. 0 means idle (no component active).
static NEXT_PROBE_ID: AtomicU32 = AtomicU32::new(1);

/// Maps component_id strings to their assigned probe group IDs so that
/// duplicate spans (e.g. builder + spawn) reuse the same ID.
static REGISTERED: Mutex<Option<HashMap<String, u32>>> = Mutex::new(None);

/// Stored in span extensions to associate a span with a probe group ID.
struct ProbeGroupId(u32);

/// Extracts the `component_id` field value from span attributes.
#[derive(Default)]
struct ComponentIdVisitor {
    component_id: Option<String>,
}

impl Visit for ComponentIdVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "component_id" {
            self.component_id = Some(value.to_owned());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "component_id" {
            self.component_id = Some(format!("{value:?}"));
        }
    }
}

/// A tracing layer that writes the active component's group ID to a per-thread
/// [`AtomicU32`] on span enter and clears it on exit.
///
/// Detects component spans via the `component_id` field in `on_new_span`,
/// assigns a unique probe group ID, and registers the mapping with bpftrace
/// via [`vector_register_component`]. Independent of `allocation-tracing`.
pub struct ComponentProbesLayer<S> {
    _subscriber: PhantomData<S>,
}

impl<S> Default for ComponentProbesLayer<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> ComponentProbesLayer<S> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            _subscriber: PhantomData,
        }
    }
}

impl<S> Layer<S> for ComponentProbesLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut visitor = ComponentIdVisitor::default();
        attrs.record(&mut visitor);

        if let Some(component_id) = visitor.component_id {
            let probe_id = {
                let mut guard = REGISTERED.lock().unwrap_or_else(|e| e.into_inner());
                let map = guard.get_or_insert_with(HashMap::new);

                if let Some(&existing) = map.get(&component_id) {
                    existing
                } else {
                    let new_id = NEXT_PROBE_ID.fetch_add(1, Ordering::Relaxed);
                    if new_id == 0 {
                        return;
                    }

                    // Null-terminate for bpftrace: the kernel's
                    // bpf_probe_read_user_str treats the length as buffer size
                    // including the null terminator, so str(ptr, len) reads at
                    // most len-1 chars. Using str(ptr) reads until the null
                    // byte, which works across all bpftrace versions.
                    let c_name = std::ffi::CString::new(component_id.as_str())
                        .expect("component_id should not contain null bytes");
                    let name_bytes = c_name.as_bytes_with_nul();
                    vector_register_component(new_id, name_bytes.as_ptr(), name_bytes.len());

                    map.insert(component_id, new_id);
                    new_id
                }
            };

            if let Some(span) = ctx.span(id) {
                span.extensions_mut().insert(ProbeGroupId(probe_id));
            }
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id)
            && let Some(probe) = span.extensions().get::<ProbeGroupId>()
        {
            thread_label().store(probe.0, Ordering::Relaxed);
            vector_component_enter(probe.0);
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id)
            && span.extensions().get::<ProbeGroupId>().is_some()
        {
            thread_label().store(0, Ordering::Relaxed);
            vector_component_exit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_label_store_and_clear() {
        let label = thread_label();
        let group_id: u32 = 7;

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
                tx.send(thread_label() as *const AtomicU32 as usize)
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

    #[test]
    fn component_enter_exit_do_not_panic() {
        vector_component_enter(42);
        vector_component_exit();
    }
}
