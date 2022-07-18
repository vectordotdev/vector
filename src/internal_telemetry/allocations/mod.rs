//! Allocation tracking exposed via internal telemetry.

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
//
// TODO: We should explore a design where we essentially bring in `tracking-allocator` directly and tweak it such that
// we collapse a majority of the various thread locals, and lean more on what we already do, specifically related to the
// span stack.
//
// Essentially, since we always need to coordinate with the span stack to enter in and out, could we use the span stack
// to actually store the (de)alloc counts and amounts and then push them as an aggregated event when the current group
// is popped off the stack? We'd still have to enter the thread local in `alloc`/`dealloc` so it might be a wash, and
// there's also the question of how we handle tasks that infrquently yield (aka would exit the span) or don't yield at
// all... then we're back in "tracked a bunch of events but never sent them" territory... but... there might be
// something we could do here *shrug*

use std::{
    cell::UnsafeCell,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use crossbeam_queue::ArrayQueue;
use crossbeam_utils::Backoff;
use once_cell::sync::OnceCell;
use slab::Slab;
use tracking_allocator::{
    AllocationGroupId, AllocationGroupToken, AllocationRegistry, AllocationTracker,
};

mod allocator;

mod channel;
use self::channel::{create_channel, Consumer, Producer};

const BATCH_SIZE: usize = 256;
const BATCHES: usize = 256;

static REGISTRATIONS: OnceCell<Registrations> = OnceCell::new();

thread_local! {
    static LOCAL_PRODUCER: UnsafeCell<Option<Producer<BATCH_SIZE, BATCHES, AllocatorEvent>>> = UnsafeCell::new(None);
}

fn get_registrations() -> &'static Registrations {
    REGISTRATIONS.get_or_init(Registrations::new)
}

//#[inline]
fn with_local_event_producer<F>(mut f: F)
where
    F: FnMut(&mut Producer<BATCH_SIZE, BATCHES, AllocatorEvent>),
{
    let _result = LOCAL_PRODUCER.try_with(|maybe_producer| {
        // SAFETY: The producer lives in a thread local, so we have guaranteed exclusive access to it, which
        // ensures our creation of a mutable reference is safe within the closure given to `LocalKey::try_with`.
        //
        // Additionally, we know the pointer-to-reference cast is safe/always valid because we just got it from `UnsafeCell`.
        let producer = unsafe {
            maybe_producer
                .get()
                .as_mut()
                .expect("producer pointer should always be valid")
                .get_or_insert_with(register_event_channel)
        };

        f(producer);
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
    Allocation,
    Deallocation,
}

struct Registrations {
    pending: ArrayQueue<Consumer<BATCH_SIZE, BATCHES, AllocatorEvent>>,
    has_pending: AtomicBool,
}

impl Registrations {
    fn new() -> Self {
        Self {
            pending: ArrayQueue::new(1024),
            has_pending: AtomicBool::new(false),
        }
    }

    fn register(&self, mut consumer: Consumer<BATCH_SIZE, BATCHES, AllocatorEvent>) {
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

    fn has_pending_registrations(&self) -> bool {
        self.has_pending.load(Ordering::Relaxed)
    }

    fn get_pending_registration(&self) -> Option<Consumer<BATCH_SIZE, BATCHES, AllocatorEvent>> {
        let result = self.pending.pop();
        if result.is_none() {
            self.has_pending.store(false, Ordering::Release);
        }

        result
    }
}

struct Collector {
    consumers: Slab<Consumer<BATCH_SIZE, BATCHES, AllocatorEvent>>,
    //consumer_empty: Vec<usize>,
    registrations: &'static Registrations,
}

impl Collector {
    fn new() -> Self {
        let registrations = get_registrations();

        Self {
            consumers: Slab::new(),
            //consumer_empty: Vec::new(),
            registrations,
        }
    }

    fn run(&mut self) {
        // Create two simple atomics for tracking the number of allocations and deallocations, and spawn a separate
        // thread to print out those values on an interval.
        let allocs = Arc::new(AtomicUsize::new(0));
        let deallocs = Arc::new(AtomicUsize::new(0));

        // Run the allocation reporter in the background.
        //run_allocation_reporter(Arc::clone(&allocs), Arc::clone(&deallocs));

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
                let mut local_allocs = 0;
                let mut local_deallocs = 0;

                let mut processor = |events: &[AllocatorEvent]| {
                    for event in events {
                        match event {
                            AllocatorEvent::Allocation { .. } => local_allocs += 1,
                            AllocatorEvent::Deallocation { .. } => local_deallocs += 1,
                        }
                    }
                };

                let mut loops = 0;
                let mut should_sleep = false;
                loop {
                    // We need to force ourselves to yield temporarily so that we can check registrations, but we make
                    // sure that we don't bother sleeping because we know if we looped this much, there's a lot of
                    // allocator activity and we don't want to starve producers for writable chunks.
                    if loops > 1000 {
                        break;
                    }

                    // Loop over every consumer, trying to consume a readable chunk if one is available.
                    let mut consumed = false;
                    for (_, consumer) in self.consumers.iter_mut() {
                        if consumer.try_consume(&mut processor).is_some() {
                            consumed = true;
                        }
                    }

                    // If we didn't get anything at all, break out early and briefly sleep.
                    if !consumed {
                        should_sleep = true;
                        break;
                    }

                    loops += 1;
                }

                allocs.fetch_add(local_allocs, Ordering::Relaxed);
                deallocs.fetch_add(local_deallocs, Ordering::Relaxed);

                if should_sleep {
                    // Sleep for a brief period.
                    thread::sleep(Duration::from_millis(10));
                }
            }
        });
    }
}

#[allow(dead_code)]
fn run_allocation_reporter(allocs: Arc<AtomicUsize>, deallocs: Arc<AtomicUsize>) {
    let alloc_reporter = thread::Builder::new().name("vector-alloc-reporter".to_string());
    alloc_reporter
        .spawn(move || loop {
            thread::sleep(Duration::from_secs(1));

            info!(
                total_allocations = allocs.load(Ordering::Relaxed),
                total_deallocations = deallocs.load(Ordering::Relaxed),
                "Allocator activity.",
            );
        })
        .unwrap();
}

struct Tracker;

impl AllocationTracker for Tracker {
    fn allocated(
        &self,
        _addr: usize,
        _object_size: usize,
        _wrapped_size: usize,
        _group_id: AllocationGroupId,
    ) {
        with_local_event_producer(|producer| {
            producer.write(AllocatorEvent::Allocation);
        });
    }

    fn deallocated(
        &self,
        _addr: usize,
        _object_size: usize,
        _wrapped_size: usize,
        _source_group_id: AllocationGroupId,
        _current_group_id: AllocationGroupId,
    ) {
        with_local_event_producer(|producer| {
            producer.write(AllocatorEvent::Deallocation);
        });
    }
}

/// Initializes allocation tracking.
pub fn init_allocation_tracking() {
    let mut collector = Collector::new();

    let alloc_processor = thread::Builder::new().name("vector-alloc-processor".to_string());
    alloc_processor.spawn(move || collector.run()).unwrap();

    AllocationRegistry::set_global_tracker(Tracker)
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
    // TODO: register the allocation group token with its tags via `Collector`: we can't do it via `Registrations`
    // because that gets checked lazily/periodically, and we need to be able to associate a group ID with its tags
    // immediately so that we don't misassociate events
    AllocationGroupToken::register().expect("failed to register allocation group token")
}
