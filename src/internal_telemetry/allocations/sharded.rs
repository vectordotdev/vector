use std::{
    ops::Deref,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};

static THREAD_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static THREAD_ID: usize = THREAD_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
}

const DEFAULT_SHARD_FACTOR: usize = 8;

#[derive(Debug)]
pub struct ShardedAtomicU64<const N: usize = DEFAULT_SHARD_FACTOR> {
    slots: [AtomicU64; N],
}

impl<const N: usize> ShardedAtomicU64<N> {
    pub const fn new() -> Self {
        assert!(N == N.next_power_of_two(), "shard factor must be a power of two");

        const DEFAULT_ATOMIC: AtomicU64 = AtomicU64::new(0);
        let slots = [DEFAULT_ATOMIC; N];

        Self { slots }
    }

    #[inline]
    fn get(&self) -> &AtomicU64 {
        let id = THREAD_ID.try_with(|id| *id).unwrap_or_default();
        let idx = id.rotate_right(N.trailing_zeros());
        &self.slots[idx]
    }

    fn get_all(&self) -> &[AtomicU64] {
        &self.slots
    }
}

impl<const N: usize> Default for ShardedAtomicU64<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Deref for ShardedAtomicU64<N> {
    type Target = AtomicU64;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}
