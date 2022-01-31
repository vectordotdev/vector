use std::{
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::Duration,
};
use thingbuf::mpsc::sync::{StaticChannel, StaticReceiver, StaticSender};
use tracking_allocator::{AllocationRegistry, AllocationTracker};

enum AllocationEvent {
    Empty,
    Allocated {
        addr: usize,
        size: usize,
        group_id: usize,
        tags: Option<&'static [(&'static str, &'static str)]>,
    },
    Deallocated {
        addr: usize,
    },
}

impl Default for AllocationEvent {
    fn default() -> Self {
        AllocationEvent::Empty
    }
}

static ALLOCATION_EVENT_QUEUE: StaticChannel<AllocationEvent, 8192> = StaticChannel::new();

struct StaticQueueTracker {
    tx: StaticSender<AllocationEvent>,
}

impl AllocationTracker for StaticQueueTracker {
    fn allocated(
        &self,
        addr: usize,
        size: usize,
        group_id: usize,
        tags: Option<&'static [(&'static str, &'static str)]>,
    ) {
        let _ = self.tx.send(AllocationEvent::Allocated {
            addr,
            size,
            group_id,
            tags,
        });
    }

    fn deallocated(&self, addr: usize) {
        let _ = self.tx.send(AllocationEvent::Deallocated { addr });
    }
}

pub fn init_allocation_tracking() {
    let (tx, rx) = ALLOCATION_EVENT_QUEUE.split();

    // Create our tracker that will push allocation events to the static queue:
    let _ = AllocationRegistry::set_global_tracker(StaticQueueTracker { tx })
        .expect("no other global tracker should be set yet");

    // Spawn our thread that does our event data processing:
    let _ = thread::spawn(move || process_allocation_events(rx));
    let _ = thread::spawn(move || report_allocations());

    // And, finally, enable tracking:
    AllocationRegistry::enable_tracking();
}

static ALLOC_COUNTS: [AtomicUsize; 16] = [
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
];

fn report_allocations() {
    loop {
        std::thread::sleep(Duration::from_secs(1));

        let counts = [
            ALLOC_COUNTS[0].load(Ordering::Relaxed),
            ALLOC_COUNTS[1].load(Ordering::Relaxed),
            ALLOC_COUNTS[2].load(Ordering::Relaxed),
            ALLOC_COUNTS[3].load(Ordering::Relaxed),
            ALLOC_COUNTS[4].load(Ordering::Relaxed),
            ALLOC_COUNTS[5].load(Ordering::Relaxed),
            ALLOC_COUNTS[6].load(Ordering::Relaxed),
            ALLOC_COUNTS[7].load(Ordering::Relaxed),
            ALLOC_COUNTS[8].load(Ordering::Relaxed),
            ALLOC_COUNTS[9].load(Ordering::Relaxed),
            ALLOC_COUNTS[10].load(Ordering::Relaxed),
            ALLOC_COUNTS[11].load(Ordering::Relaxed),
            ALLOC_COUNTS[12].load(Ordering::Relaxed),
            ALLOC_COUNTS[13].load(Ordering::Relaxed),
            ALLOC_COUNTS[14].load(Ordering::Relaxed),
            ALLOC_COUNTS[15].load(Ordering::Relaxed),
        ];

        println!("alloc counts: 0={} 1={} 2={} 3={} 4={} 5={} 6={} 7={} 8={} 9={} 10={} 11={} 12={} 13={} 14={} 15={} ",
            counts[0], counts[1], counts[2], counts[3], counts[4], counts[5], counts[6], counts[7],
            counts[8], counts[9], counts[10], counts[11], counts[12], counts[13], counts[14], counts[15])
    }
}

fn process_allocation_events(rx: StaticReceiver<AllocationEvent>) {
    while let Some(event) = rx.recv() {
        match event {
            AllocationEvent::Empty => unreachable!(),
            AllocationEvent::Allocated { group_id, .. } => {
                if group_id < 16 {
                    ALLOC_COUNTS[group_id].fetch_add(1, Ordering::Relaxed);
                }
                //println!(
                //    "allocation -> addr={:#x} size={} group_id={} tags={:?}",
                //    addr, size, group_id, tags
                //);
            }
            AllocationEvent::Deallocated { .. } => {
                //println!("deallocation -> addr={:#x}", addr);
            }
        }
    }
}
