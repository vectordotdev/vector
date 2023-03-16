use std::{
    ops::Deref,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};

static THREAD_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static THREAD_ID: usize = THREAD_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
}

const DEFAULT_SHARD_FACTOR: usize = 8;
const SHARD_FACTOR_MASK: usize = DEFAULT_SHARD_FACTOR - 1;

#[derive(Debug)]
pub struct ShardedAtomicU64 {
    slots: [AtomicU64; DEFAULT_SHARD_FACTOR],
}

impl ShardedAtomicU64 {
    pub const fn new() -> Self {
        assert!(
            DEFAULT_SHARD_FACTOR == DEFAULT_SHARD_FACTOR.next_power_of_two(),
            "shard factor must be a power of two"
        );
        assert!(
            SHARD_FACTOR_MASK == DEFAULT_SHARD_FACTOR - 1,
            "shard factor mask must be N-1 (i.e. factor = 8 (0x1000), mask = 7 (0x111)"
        );

        const DEFAULT_ATOMIC: AtomicU64 = AtomicU64::new(0);
        let slots = [DEFAULT_ATOMIC; DEFAULT_SHARD_FACTOR];

        Self { slots }
    }

    fn get(&self) -> &AtomicU64 {
        let id = THREAD_ID.try_with(|id| *id).unwrap_or_default();
        let idx = id & SHARD_FACTOR_MASK;
        &self.slots[idx]
    }

    pub fn get_all(&self) -> &[AtomicU64] {
        &self.slots
    }
}

impl Default for ShardedAtomicU64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for ShardedAtomicU64 {
    type Target = AtomicU64;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}
