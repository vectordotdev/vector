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

use std::{cell::UnsafeCell, sync::atomic::{AtomicBool, Ordering}, num::NonZeroUsize, thread, time::Duration};

use arrayvec::ArrayVec;
use crossbeam_queue::ArrayQueue;
use crossbeam_utils::Backoff;
use once_cell::sync::OnceCell;
use slab::Slab;
use tracking_allocator::{
    AllocationGroupId, AllocationGroupToken, AllocationRegistry, AllocationTracker,
};
use super::channel::{Consumer, Producer, create_channel};

const BATCH_SIZE: usize = 64;
const BATCHES: usize = 8192;

static REGISTRATIONS: OnceCell<Registrations> = OnceCell::new();

thread_local! {
    static LOCAL_PRODUCER: UnsafeCell<Option<Producer<BATCH_SIZE, BATCHES, AllocatorEvent>>> = UnsafeCell::new(None);
    static LOCAL_EVENT_BUFFER: UnsafeCell<ArrayVec<AllocatorEvent, BATCH_SIZE>> = UnsafeCell::new(ArrayVec::new_const());
}

fn get_registrations() -> &'static Registrations {
    REGISTRATIONS.get_or_init(|| Registrations::new())
}

#[inline]
fn with_local_event_producer<F>(f: F)
where
    F: Fn(&mut Producer<BATCH_SIZE, BATCHES, AllocatorEvent>),
{
    LOCAL_PRODUCER.try_with(|maybe_producer| {
        // SAFETY: The producer lives in a thread local, so we have guaranteed exclusive access to it, which
        // ensures our creation of a mutable reference is safe within the closure given to `LocalKey::try_with`.
        //
        // Additionally, we know the pointer-to-reference cast is safe/always valid because we just got it from `UnsafeCell`.
        let producer = unsafe {
            maybe_producer.get()
                .as_mut()
                .expect("producer pointer should always be valid")
                .get_or_insert_with(register_event_channel)
        };

        f(producer);
    });
}

#[inline]
fn with_local_event_buffer<F>(f: F)
where
    F: Fn(&mut ArrayVec<AllocatorEvent, BATCH_SIZE>),
{
    LOCAL_EVENT_BUFFER.try_with(|raw_local_event_buffer| {
        // SAFETY: The local event buffer lives in a thread local, so we have guaranteed exclusive access to it, which
        // ensures our creation of a mutable reference is safe within the closure given to `LocalKey::try_with`.
        //
        // Additionally, we know the pointer-to-reference cast is safe/always valid because we just got it from `UnsafeCell`.
        let local_event_buffer = unsafe {
            raw_local_event_buffer.get()
                .as_mut()
                .expect("local event buffer pointer should always be valid")
        };

        f(local_event_buffer);
    });
}

#[inline]
fn buffer_allocator_event(event: AllocatorEvent) {
    with_local_event_buffer(|local_event_buffer| {
        loop {
            match local_event_buffer.try_push(event) {
                // There was space to buffer the event, so we're done.
                Ok(()) => break,
                // There wasn't enough space, so we need to flush our buffer to the local producer and try again.
                Err(e) => {
                    event = e.element();

                    with_local_event_producer(|producer| {
                        let chunk = producer.acquire_chunk();
                        match chunk.try_write(&local_event_buffer) {
                            None => {
                                // We wrote all buffered events, so we can clear our local buffer entirely.
                                local_event_buffer.clear();
                            },
                            Some(written) => {
                                // We had a partial write, so pop off the events we managed to send.
                                for _ in 0..written {
                                    local_event_buffer.pop_at(0).expect("buffered event should exist if we wrote it");
                                }
                            }
                        }
                    });
                }
            }
        }
    });
}

fn register_event_channel() -> Producer<BATCH_SIZE, BATCHES, AllocatorEvent> {
    let (producer, consumer) = create_channel();
    let registrations = get_registrations();
    registrations.register(consumer);

    producer
}

#[derive(Clone, Copy)]
enum AllocatorEvent {
    Allocation {
        group_id: NonZeroUsize,
        wrapped_size: usize,
    },

    Deallocation {
        source_group_id: NonZeroUsize,
        wrapped_size: usize,
    }
}

struct Registrations {
    pending: ArrayQueue<Consumer<BATCH_SIZE, BATCHES, AllocatorEvent>>,
    has_pending: AtomicBool,
}

impl Registrations {
    pub fn new() -> Self {
        Self {
            pending: ArrayQueue::new(1024),
            has_pending: AtomicBool::new(false),
        }
    }

    pub fn register(&self, mut consumer: Consumer<BATCH_SIZE, BATCHES, AllocatorEvent>) {
        // Try sending the consumer to the collector until we succeed, as it should be checking
        // `has_pending_registrations` to see if there's anything to process and then following up quickly... so if
        // we're waiting here for a slot to open, all we can really do is wait out the blockade.
        let backoff = Backoff::new();
        while let Err(old_consumer) = self.pending.push(consumer) {
            backoff.snooze();
            consumer = old_consumer;
        }

        // Once we add it to the pending queue, we now mark `has_pending` for real.
        self.has_pending.store(true, Ordering::Release);
    }

    pub fn has_pending_registrations(&self) -> bool {
        self.has_pending.load(Ordering::Relaxed)
    }

    pub fn get_pending_registration(&self) -> Option<Consumer<BATCH_SIZE, BATCHES, AllocatorEvent>> {
        let result = self.pending.pop();
        if result.is_none() {
            self.has_pending.store(false, Ordering::Release);
        }

        result
    }
}

struct Collector {
    consumers: Slab<Consumer<BATCH_SIZE, BATCHES, AllocatorEvent>>, 
    consumer_empty: Vec<usize>,
    registrations: &'static Registrations,
}

impl Collector {
    fn new() -> Self {
        let registrations = get_registrations();

        Self { consumers: Slab::new(), consumer_empty: Vec::new(), registrations }
    }

    fn run(&mut self) {
        // We don't want to track allocator events here, because speed is the name of the game, and also, things could
        // potentially get into a not-so-great feedback loop.
        AllocationRegistry::untracked(|| {
            loop {
                // Check if any consumers are pending registration.
                if self.registrations.has_pending_registrations() {
                    while let Some(consumer) = self.registrations.get_pending_registration() {
                        self.consumers.insert(consumer);
                    }
                }

                // Process all consumers, getting all outstanding events. We loop through every consumer at least once, and
                // for any consumer that we get events back from, we'll try to immediately consume from it again. We don't
                // consume until nothing is left, because we might otherwise get bottlenecked on a super busy consumer. We
                // want to make sure that we service registrations in a timely fashion, because while we don't need to
                // register a consumer before anything can be produced, the clock is one as soon as the registration is
                // pending, and we need to register and start consuming before the channel fills up, which would then really
                // screw things up for that thread. All allocations would be blocked, which is not good. 
                let processor = |events: &[AllocatorEvent]| {
                    println!("got {} events", events.len());
                };

                for (_, consumer) in self.consumers.iter_mut() {
                    if let Some(_) = consumer.try_consume(processor) {
                        consumer.try_consume(processor);
                    }
                }

                // Sleep for a brief period.
                thread::sleep(Duration::from_millis(10));
            }
        });
    }
}

struct Tracker;

impl AllocationTracker for Tracker {
    fn allocated(
        &self,
        _addr: usize,
        _object_size: usize,
        wrapped_size: usize,
        group_id: AllocationGroupId,
    ) {
        let mut event = AllocatorEvent::Allocation {
            group_id: group_id.as_usize(),
            wrapped_size,
        };

        buffer_allocator_event(event);
    }

    fn deallocated(
        &self,
        _addr: usize,
        _object_size: usize,
        wrapped_size: usize,
        source_group_id: AllocationGroupId,
        _current_group_id: AllocationGroupId,
    ) {
        let mut event = AllocatorEvent::Deallocation {
            source_group_id: source_group_id.as_usize(),
            wrapped_size,
        };

        buffer_allocator_event(event);
    }
}

/// Initializes allocation tracking.
pub fn init_allocation_tracking() {
    let collector = Collector::new();
    thread::spawn(move || collector.run());

    let _ = AllocationRegistry::set_global_tracker(Tracker)
        .expect("no other global tracker should be set yet");

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
pub fn acquire_allocation_group_token(_tags: Vec<(String, String)>) -> AllocationGroupToken {
    let token =
        AllocationGroupToken::register().expect("failed to register allocation group token");

    //let collector = get_global_collector();
    //collector.register(token.id());

    // BROKEN: currently, we register a token on the thread it's created, which initializes the page it needs, etc
    // etc... but what we actually need to do is initialize it in all threads, or know that it isn't already initialized
    // in the current thread and do so... but that implies tracking a lot of per-thread state which might get squicky,
    // and would mean a runtime check per tracked allocation, which kinda sucks...
    //
    // this might mean that we actually want to do at least the `is_initialized`/`initialize` call automatically in
    // `LazilyAllocatedPage::get` if we can make sure it's super cheap, and then we can make that method much safer

    token
}
