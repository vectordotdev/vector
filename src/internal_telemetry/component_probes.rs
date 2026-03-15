//! Lightweight bpftrace-based per-component CPU attribution.
//!
//! When the `component-probes` feature is enabled, this module provides a
//! [`ComponentProbesLayer`] that tags each Tokio worker thread with the ID of
//! the currently executing component. External bpftrace scripts read this tag
//! on a profile timer to produce per-component flamegraphs.

use std::{
    marker::PhantomData,
    sync::atomic::{AtomicU32, Ordering},
};

use tracing::{
    Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id},
};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

/// Returns a leaked `&'static AtomicU32` unique to the current thread.
///
/// On first access, allocates a byte via `Box::leak` and calls
/// [`vector_register_thread`] so bpftrace can map this thread's TID
/// to the byte's address. The leaked byte is valid for the process lifetime.
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
pub extern "C" fn vector_register_component(
    id: u32,
    name_ptr: *const u8,
    name_len: usize,
) {
    std::hint::black_box((id, name_ptr, name_len));
}

/// Next probe group ID. 0 means idle (no component active).
static NEXT_PROBE_ID: AtomicU32 = AtomicU32::new(1);

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
            let probe_id = NEXT_PROBE_ID.fetch_add(1, Ordering::Relaxed);
            if probe_id == 0 {
                return;
            }

            let id_bytes = component_id.as_bytes();
            vector_register_component(probe_id, id_bytes.as_ptr(), id_bytes.len());

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
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id)
            && span.extensions().get::<ProbeGroupId>().is_some()
        {
            thread_label().store(0, Ordering::Relaxed);
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
}
