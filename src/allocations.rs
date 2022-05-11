//! Allocation tracking exposed via internal telemetry.

// TODO: We need to make the late registration thread update group entries after it registers them in `metrics`,
// otherwise we risk futures changes -- such as switching to generational storage -- breaking how these metrics are
// collected if we're not going through the normal `Counter` interface.
//
// TODO: We don't currently register the root allocation group which means we're missing all the allocations that happen
// outside of the various component tasks. Additionally, we are likely missing span propagation for spawned tasks that
// occur under component tasks. Likely, not for sure... but likely.
//
// TODO: Another big chunk of memory use at low load will be thread stacks, especially with how big/deep futures
// generators are. Thread stacks can be large: 8MB by default on Linux. This means that if we use a lot of blocking
// tasks, we're at least making calls for 8MB chunks for each thread, which is still virtual memory so it's not actually
// physically wired in all at once... but it's a large potential lever on memory usage, again, with how big/deep futures
// generators are.
//
// This is made harder to track because, for example, on Linux, `pthread_create` is using `mmap -- a syscall -- to get a
// private anonymous region for a thread's stack, so we can't even capture that allocation in our user-mode global
// allocator.
//
// TODO: Maybe we should track VSZ/RSS overall for the process so that we can at least emit it alongside the allocation
// metrics to have more of a full picture.. as you could intuit from the above TODOs, the numbers may still diverge
// quite a bit but they should all be correlated/directional enough to tell the full story.
//
// TODO: Could we take a reference to the span that we want to attach the allocation group token to, and then visit all
// of the fields to automatically extract the relevant metric tags? We could then also attach the token to the span for
// the caller, so that they never even needed to bother doing that. This would be cleaner than having to generate the
// vector of tags by hand, which is obviously a "do it once and then never change it" thing but would look a lot cleaner
// in this proposed method.
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use arc_swap::ArcSwapOption;
use metrics::Key;
use once_cell::sync::OnceCell;
use thingbuf::mpsc::blocking::{StaticChannel, StaticReceiver, StaticSender};
use tracking_allocator::{
    AllocationGroupId, AllocationGroupToken, AllocationRegistry, AllocationTracker,
};
use vector_core::metrics::{Controller, Handle};

static ALLOCATION_LUT: ArcSwapOption<Vec<Option<AllocationGroupEntry>>> =
    ArcSwapOption::const_empty();
static REGISTRATION_EVENTS: OnceCell<RegistrationEvents> = OnceCell::new();

struct RegistrationEvents {
    tx: StaticSender<Option<RegistrationEvent>>,
    rx: StaticReceiver<Option<RegistrationEvent>>,
}

impl RegistrationEvents {
    fn from_channel<const N: usize>(
        channel: &'static StaticChannel<Option<RegistrationEvent>, N>,
    ) -> Self {
        let (tx, rx) = channel.split();
        Self { tx, rx }
    }

    fn push(&self, event: RegistrationEvent) {
        self.tx
            .send(Some(event))
            .expect("received is static, and can never drop/disconnect")
    }

    fn pop(&self) -> RegistrationEvent {
        self.rx
            .recv()
            .expect("sender is static, and can never drop/disconnect")
            .expect("should never recv a legitimate None")
    }
}

#[derive(Clone)]
struct RegistrationEvent {
    group_entry: AllocationGroupEntry,
    tags: Vec<(String, String)>,
}

#[derive(Clone)]
struct AllocationGroupEntry {
    allocated_bytes: Arc<AtomicU64>,
    deallocated_bytes: Arc<AtomicU64>,
    allocations: Arc<AtomicU64>,
    deallocations: Arc<AtomicU64>,
}

impl AllocationGroupEntry {
    fn new() -> Self {
        Self {
            allocated_bytes: Arc::new(AtomicU64::new(0)),
            deallocated_bytes: Arc::new(AtomicU64::new(0)),
            allocations: Arc::new(AtomicU64::new(0)),
            deallocations: Arc::new(AtomicU64::new(0)),
        }
    }

    fn track_allocation(&self, bytes: u64) {
        self.allocated_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.allocations.fetch_add(1, Ordering::Relaxed);
    }

    fn track_deallocation(&self, bytes: u64) {
        self.deallocated_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.deallocations.fetch_add(1, Ordering::Relaxed);
    }

    const fn allocated_bytes(&self) -> &Arc<AtomicU64> {
        &self.allocated_bytes
    }

    const fn deallocated_bytes(&self) -> &Arc<AtomicU64> {
        &self.deallocated_bytes
    }

    const fn allocations(&self) -> &Arc<AtomicU64> {
        &self.allocations
    }

    const fn deallocations(&self) -> &Arc<AtomicU64> {
        &self.deallocations
    }
}

struct Tracker;

impl AllocationTracker for Tracker {
    fn allocated(&self, _addr: usize, size: usize, group_id: AllocationGroupId) {
        with_allocation_group_entry(group_id, |entry| entry.track_allocation(size as u64));
    }

    fn deallocated(
        &self,
        _addr: usize,
        size: usize,
        source_group_id: AllocationGroupId,
        _current_group_id: AllocationGroupId,
    ) {
        with_allocation_group_entry(source_group_id, |entry| {
            entry.track_deallocation(size as u64)
        });
    }
}

/// Initializes allocation tracking.
///
/// This sets the global allocation tracker to our custom tracker implementation, spawns a background thread which
/// handles registering allocation groups by attaching their atomic counters to our internal metrics backend, and then
/// finally enables tracking which causes (de)allocation events to begin flowing.
pub fn init_allocation_tracking() {
    let _ = AllocationRegistry::set_global_tracker(Tracker {})
        .expect("no other global tracker should be set yet");

    thread::spawn(process_registration_events);

    AllocationRegistry::enable_tracking();
}

/// Acquires an allocation group token.
///
/// This creates an allocation group which allows callers to enter/exit the allocation group context, associating all
/// (de)allocations within the context with that group.  That token can (and typically is) associated with a
/// /// `tracing::Span` such that the context is entered and exited as the span is entered and exited. This allows
/// ensuring that we track all (de)allocations when the span is active.
///
/// # Tags
///
/// The provided `tags` are used for the metrics that get registered and attached to the allocation group. No tags from
/// the traditional `metrics`/`tracing` are collected, as the metrics are updated directly rather than emitted via the
/// traditional `metrics` macros, so the given tags should match the span fields that would traditionally be set for a
/// given span in order to ensure that they match.
pub fn acquire_allocation_group_token(tags: Vec<(String, String)>) -> AllocationGroupToken {
    let token =
        AllocationGroupToken::register().expect("failed to register allocation group token");
    let group_id = token.id();

    // Register the atomic counters, etc, for this group token.
    let group_entry = register_allocation_group_token_entry(&group_id);

    // Send the group ID and entry to our late registration thread so that it can correctly wire up any allocation
    // groups to our metrics backend once it's been initialized.
    let registration_events = get_registration_events();
    registration_events.push(RegistrationEvent { group_entry, tags });

    token
}

fn register_allocation_group_token_entry(group_id: &AllocationGroupId) -> AllocationGroupEntry {
    AllocationRegistry::untracked(|| {
        let group_id = group_id.as_usize().get();
        let group_entry = AllocationGroupEntry::new();

        // Create our allocated/deallocated counters and store them at their respective index in the global lookup
        // table. This requires us to (potentially) resize the vector and fill it with empty values if we're racing
        // another acquisition that is behind us, ID-wise.
        ALLOCATION_LUT.rcu(|lut| {
            let mut lut = lut.as_ref()
                .map(|a| a.as_ref().clone())
                .unwrap_or_default();

            // Make sure the vector is long enough that we can directly index our group ID.
            let minimum_len = group_id + 1;
            if lut.len() < minimum_len {
                lut.resize(minimum_len, None);
            }

            {
                let entry = unsafe { lut.get_unchecked_mut(group_id) };
                if entry.is_some() {
                    panic!("allocation LUT entry was already populated for newly acquired allocation group token!");
                }

                *entry = Some(group_entry.clone());
            }

            Some(Arc::new(lut))
        });

        group_entry
    })
}

fn with_allocation_group_entry<F>(group_id: AllocationGroupId, f: F) -> bool
where
    F: FnOnce(&AllocationGroupEntry),
{
    let lut = ALLOCATION_LUT.load();
    if let Some(inner) = lut.as_ref() {
        if let Some(Some(entry)) = inner.get(group_id.as_usize().get()) {
            f(entry);
            true
        } else {
            false
        }
    } else {
        false
    }
}

fn get_registration_events() -> &'static RegistrationEvents {
    REGISTRATION_EVENTS.get_or_init(|| {
        static CHANNEL: StaticChannel<Option<RegistrationEvent>, 128> = StaticChannel::new();

        RegistrationEvents::from_channel(&CHANNEL)
    })
}

fn process_registration_events() {
    AllocationRegistry::untracked(|| {
        // We need to wait until our metrics backend is initialized so that we can meaningfully register our allocation
        // groups, as we can't do so until we can get a reference to the global metrics controller.
        let controller = loop {
            match Controller::get() {
                Ok(controller) => break controller,
                Err(_) => thread::sleep(Duration::from_millis(100)),
            }
        };

        // Now that we have a reference to the controller, process any existing registration events, and any future
        // ones.
        let registration_events = get_registration_events();
        loop {
            let event = registration_events.pop();

            register_allocation_group_counter(
                controller,
                &event,
                "component_allocated_bytes_total",
                |e| e.allocated_bytes(),
            );
            register_allocation_group_counter(
                controller,
                &event,
                "component_deallocated_bytes_total",
                |e| e.deallocated_bytes(),
            );
            register_allocation_group_counter(
                controller,
                &event,
                "component_allocations_total",
                |e| e.allocations(),
            );
            register_allocation_group_counter(
                controller,
                &event,
                "component_deallocations_total",
                |e| e.deallocations(),
            );
        }
    });
}

fn register_allocation_group_counter<F>(
    controller: &Controller,
    event: &RegistrationEvent,
    name: &'static str,
    get_handle: F,
) where
    F: Fn(&AllocationGroupEntry) -> &Arc<AtomicU64>,
{
    let key = Key::from_parts(name, &event.tags);
    let handle = Handle::Counter(Arc::clone(get_handle(&event.group_entry)).into());
    controller.register_handle(&key, handle);
}
