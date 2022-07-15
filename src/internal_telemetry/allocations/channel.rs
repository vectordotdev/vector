use std::mem::{MaybeUninit, self};
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const WRITABLE: bool = false;
const READABLE: bool = true;

/// A fixed-size data chunk.
///
/// This is specifically aligned to 64 bytes, and uses a fixed-size layout, to optimize for performance.
#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct Chunk<const CHUNK_LEN: usize, T: Copy>([MaybeUninit<T>; CHUNK_LEN]);

impl<const CHUNK_LEN: usize, T: Copy> Chunk<CHUNK_LEN, T> {
    /// Creates a new `Chunk`.
    pub const fn new() -> Self {
        // SAFETY: We're initializing an array of `MaybeUninit<..>` elements, which themselves need no initialization,
        // making this safe to perform.
        unsafe { Self(MaybeUninit::uninit().assume_init()) }
    }

    /// Gets a mutable reference to the underlying data.
    ///
    /// ## Safety
    ///
    /// The caller is responsible for ensuring that they have exclusive access to this chunk, as the mutable reference
    /// is created in a way that circumvents normal aliasing rules.
    #[inline]
    unsafe fn as_mut(&self) -> &mut [T] {
        slice::from_raw_parts_mut(self.0.as_ptr() as *mut _, CHUNK_LEN)
    }

    /// Gets a reference to the underlying data.
    ///
    /// The slice reference is created with the given `len`.
    ///
    /// ## Safety
    ///
    /// The caller is responsible for ensuring that they have exclusive access to this chunk, as the reference
    /// is created in a way that circumvents normal aliasing rules.
    ///
    /// As well, the caller is responsible for ensuring that the length given is valid for the data written to the chunk
    /// by the producer. If the length caused a slice reference to be created that extended past the number of elements
    /// written, even if it was still in bounds in terms of the chunk size, it would constitute immediate UB, even if
    /// somehow `T` could be represented by the underlying data in the backing allocation.
    #[inline]
    unsafe fn as_bounded_slice(&self, len: usize) -> &[T] {
        slice::from_raw_parts(self.0.as_ptr() as *const _, len)
    }
}

pub struct Inner<const CHUNK_LEN: usize, const CHUNKS: usize, T: Copy> {
    /// Individual chunks.
    chunks: [Chunk<CHUNK_LEN, T>; CHUNKS],

    /// Chunk state.
    ///
    /// This simply tracks whether or not a chunk is available for a producer to write into, or if it's been written to
    /// and is waiting for a consumer to read it.
    chunk_state: [AtomicBool; CHUNKS],

    /// Length of data in each chunk.
    ///
    /// Writers update this when marking a chunk as readable so that a valid slice reference to the data in the chunk
    /// can be reconstructed.
    ///
    /// TODO: Should we actually push this into the chunk itself? It'd be a lot more fool-proof that way, but I'm not
    /// sure if it would have some deleterious effect on performance... can't imagine it would, since it never sees any
    /// atomic content, but who knows.
    chunk_lens: [AtomicUsize; CHUNKS],

    /// Whether or not the producer is closed.
    producer_closed: AtomicBool,
}

impl<const CHUNK_LEN: usize, const CHUNKS: usize, T: Copy> Inner<CHUNK_LEN, CHUNKS, T> {
    /// Creates a new `Inner`.
    pub fn new() -> Self {
        // Create our chunk state and chunk lengths arrays manually since atomics aren't `Copy`, so we can't just use
        // the normal array initializer approach where it copies the element for each index.
        //
        // SAFETY: `MaybeUninit` does not require initialization, so we can create these arrays safely. We also ensure
        // every atomic is initialized, so we're safe to initialize the arrays. Finally, it's safe to transmute because
        // `MaybeUninit<T>` has an identical layout to `T`.
        let mut raw_chunk_state: [MaybeUninit<AtomicBool>; CHUNKS] = unsafe { MaybeUninit::uninit().assume_init() };
        for element in &mut raw_chunk_state[..] {
            element.write(AtomicBool::new(WRITABLE));
        }
        let chunk_state = unsafe { mem::transmute::<_, [AtomicBool; CHUNKS]>(raw_chunk_state) };

        let mut raw_chunk_lens: [MaybeUninit<AtomicUsize>; CHUNKS] = unsafe { MaybeUninit::uninit().assume_init() };
        for element in &mut raw_chunk_lens[..] {
            element.write(AtomicUsize::new(0));
        }
        let chunk_lens = unsafe { mem::transmute::<_, [AtomicUsize; CHUNKS]>(raw_chunk_lens) };
    
        Self {
            chunks: [Chunk::<CHUNK_LEN, T>::new(); CHUNKS],
            chunk_state,
            chunk_lens,
            producer_closed: AtomicBool::new(false),
        }
    }

    /// Marks the producer as closed.
    pub fn mark_producer_closed(&self) {
        self.producer_closed.store(true, Ordering::Release);
    }

    /// Returns whether or not the producer is closed.
    pub fn is_producer_closed(&self) -> bool {
        self.producer_closed.load(Ordering::Acquire)
    }

    #[inline]
    fn try_find_readable_chunk_pos(&self) -> Option<usize> {
        for idx in 0..CHUNKS {
            // SAFETY: We only get `idx` from iterating from 0 to `CHUNKS`, and `self.chunk_state` is a fixed-size
            // array of size `CHUNKS`, so we know the index will never be out-of-bounds.
            unsafe {
                if self.chunk_state.get_unchecked(idx).load(Ordering::Acquire) == READABLE {
                    return Some(idx);
                }
            }
        }

        None
    }

    #[inline]
    fn try_find_writable_chunk_pos(&self) -> Option<usize> {
        for idx in 0..CHUNKS {
            // SAFETY: We only get `idx` from iterating from 0 to `CHUNKS`, and `self.chunk_state` is a fixed-size
            // array of size `CHUNKS`, so we know the index will never be out-of-bounds.
            unsafe {
                if self.chunk_state.get_unchecked(idx).load(Ordering::Acquire) == WRITABLE {
                    return Some(idx);
                }
            }
        }

        None
    }

    /// Attempts to find a readable chunk.
    ///
    /// If a readable chunk is found, `Some(ChunkReader)` is returned, which wraps read access to the chunk with a
    /// guard, `ChunkReader`, that provides convenience methods for reading the chunk, as well as ensuring
    /// the chunk is always returned to the producer when it is no longer being reference.
    #[inline]
    fn try_find_readable_chunk(&self) -> Option<(usize, &[T])> {
        self.try_find_readable_chunk_pos().map(|idx| {
            let data_len = self.chunk_lens[idx].load(Ordering::Acquire);

            // SAFETY: We take a reference to the chunk data only if the chunk is readable, which
            // implies the chunk won't be accessed for writing (chunks are read XOR write). The arrangement of
            // `Inner` ensures that there is only ever one consumer attached to a given `Inner`, so we know that
            // we do not have to worry about another call to `try_find_readable_chunk` taking a
            // reference to this chunk.
            (idx, unsafe { self.chunks[idx].as_bounded_slice(data_len) })
        })
    }

    /// Attempts to find a writable chunk.
    ///
    /// If a writable chunk is found, `Some(ChunkWriter)` is returned, which wraps write access to the chunk with a
    /// guard, `ChunkWriter`, that provides convenience methods for writing and sending the chunk, as well as ensuring
    /// the chunk is always returned to the consumer by the time the chunk writer is dropped.
    #[inline]
    fn try_find_writable_chunk(&self) -> Option<(usize, &mut [T])> {
        self.try_find_writable_chunk_pos().map(|idx| {
            // SAFETY: We take a mutable reference to the chunk data only if the chunk is writable, which
            // implies the chunk won't be accessed for reading (chunks are read XOR write). The arrangement of
            // `Inner` ensures that there is only ever one producer attached to a given `Inner`, so we know that
            // we do not have to worry about another call to `try_find_writable_chunk` taking a mutable
            // reference to this chunk.
            (idx, unsafe { self.chunks[idx].as_mut() })
        })
    }

    /// Marks a chunk as readable.
    ///
    /// This also updates the chunk length so that a valid slice reference can be created when the chunk is read by the consumer.
    #[inline]
    fn mark_chunk_readable(&self, idx: usize, len: usize) {
        self.chunk_lens[idx].store(len, Ordering::Relaxed);
        self.chunk_state[idx].store(READABLE, Ordering::Release);
    }

    /// Marks a chunk as writable.
    #[inline]
    fn mark_chunk_writable(&self, idx: usize) {
        self.chunk_state[idx].store(WRITABLE, Ordering::Release);
    }
}

pub struct Producer<const CHUNK_LEN: usize, const CHUNKS: usize, T>
where
    T: Copy + 'static,
{
    inner: &'static Inner<CHUNK_LEN, CHUNKS, T>,
}

impl<const CHUNK_LEN: usize, const CHUNKS: usize, T> Producer<CHUNK_LEN, CHUNKS, T>
where
    T: Copy + 'static,
{
    /// Acquires a chunk for writing.
    ///
    /// This method will search indefinitely for the next available writable chunk, so consumers should ensure that they
    /// consume readable chunks as fast as reasonably possible to reduce producer waiting.
    ///
    /// The chunk is returned in a guard, `ChunkWriter`, which ensures mutable access to the chunk as well as ensuring
    /// the chunk is sent (if it wasn't already) when the guard is dropped.
    #[inline]
    pub fn acquire_chunk(&mut self) -> ChunkWriter<'_, CHUNK_LEN, CHUNKS, T> {
        loop {
            if let Some((idx, chunk)) = self.inner.try_find_writable_chunk() {
                return ChunkWriter {
                    producer: self,
                    idx,
                    written: 0,
                    chunk,
                }
            }
        }
    }

    /// Marks a chunk as readable.
    #[inline]
    fn mark_chunk_readable(&self, idx: usize, len: usize) {
        self.inner.mark_chunk_readable(idx, len)
    }

    fn mark_closed(&mut self) {
        self.inner.producer_closed.store(true, Ordering::Release);
    }
}

impl<const CHUNK_LEN: usize, const CHUNKS: usize, T> Drop for Producer<CHUNK_LEN, CHUNKS, T>
where
    T: Copy + 'static,
{
    fn drop(&mut self) {
        self.mark_closed();
    }
}

/// RAII structure used to provide exclusive write access to a [`Chunk`].
///
/// When the guard is dropped, the chunk will be sent to the consumer if it was not already.
pub struct ChunkWriter<'a, const CHUNK_LEN: usize, const CHUNKS: usize, T>
where
    T: Copy + 'static,
{
    producer: &'a mut Producer<CHUNK_LEN, CHUNKS, T>,
    chunk: &'a mut [T],
    idx: usize,
    written: usize,
}

impl<'a, const CHUNK_LEN: usize, const CHUNKS: usize, T> ChunkWriter<'a, CHUNK_LEN, CHUNKS, T>
where
    T: Copy + 'static,
{
    /// Attempts to write data into the chunk.
    ///
    /// If the chunk has remaining capacity, `try_write` will write as many elements from `data` as it can, and return
    /// `Some(usize)`, containing the number of elements it was able to write. If there was no remaining capacity,
    /// `None` will be returned.
    ///
    /// Even if the write attempt fills the remaining capacity of the chunk, or if there was no remaining capacity at
    /// the time of the call, the chunk will not be automatically sent to the consumer. Callers must use `send`, or must
    /// drop the guard, to send the chunk.
    pub fn try_write(&mut self, data: impl AsRef<[T]>) -> Option<usize> {
        // See if we have capacity left in this chunk, and figure out how many elements we can write based on the
        // available capacity and the size of the data slice we've been given.
        let available = CHUNK_LEN - self.written;
        if available == 0 {
            return None;
        }

        let elements_to_write = available.min(data.as_ref().len());

        // Copy the data over.
        let (_, remaining) = self.chunk.split_at_mut(available);
        let writable_data = &data.as_ref()[..elements_to_write];
        remaining.copy_from_slice(writable_data);

        // Track how many elements were written and inform the caller.
        self.written += elements_to_write;
        Some(elements_to_write)
    }

    /// Sends this chunk to the consumer, consuming the guard.
    pub fn send(mut self) {
        self.send_inner();
    }

    fn send_inner(&mut self) {
        // Mark the chunk as readable, which stores the data length (so that the consumer doesn't read stale entries)
        // and sets the chunk state so the consumer knows it is readable.
        self.producer.mark_chunk_readable(self.idx, self.written);
    }
}

impl<'a, const CHUNK_LEN: usize, const CHUNKS: usize, T> Drop
    for ChunkWriter<'a, CHUNK_LEN, CHUNKS, T>
where
    T: Copy + 'static,
{
    fn drop(&mut self) {
        self.send_inner();
    }
}

pub struct Consumer<const CHUNK_LEN: usize, const CHUNKS: usize, T>
where
    T: Copy + 'static,
{
    inner: &'static Inner<CHUNK_LEN, CHUNKS, T>,
}

impl<const CHUNK_LEN: usize, const CHUNKS: usize, T> Consumer<CHUNK_LEN, CHUNKS, T>
where
    T: Copy + 'static,
{
    /// Attempts to consume data from the channel.
    ///
    /// If a readable chunk is found, the given closure will be called with a reference to the chunk data. After the
    /// closure returns, the chunk will be released back for use by the producer, and `Some(O)` will be returned,
    /// containing the result of the closure.
    ///
    /// Otherwise, `None` is returned, indicating that no readable chunk was found.
    pub fn try_consume<F, O>(&mut self, mut f: F) -> Option<O>
    where
        F: FnMut(&[T]) -> O,
    {
        self.inner
            .try_find_readable_chunk()
            .map(|(idx, chunk)| ChunkReader {
                consumer: self,
                idx,
                chunk,
            })
            .map(|reader| f(reader.as_ref()))
    }

    /// Marks a chunk as writable.
    #[inline]
    fn mark_chunk_writable(&self, idx: usize) {
        self.inner.mark_chunk_writable(idx)
    }
}

/// RAII structure used to provide exclusive read access to a [`Chunk`].
///
/// When the guard is dropped, the chunk will be sent back to the producer.
pub struct ChunkReader<'a, const CHUNK_LEN: usize, const CHUNKS: usize, T>
where
    T: Copy + 'static,
{
    consumer: &'a mut Consumer<CHUNK_LEN, CHUNKS, T>,
    idx: usize,
    chunk: &'a [T],
}

impl<'a, const CHUNK_LEN: usize, const CHUNKS: usize, T> AsRef<[T]>
    for ChunkReader<'a, CHUNK_LEN, CHUNKS, T>
where
    T: Copy + 'static,
{
    fn as_ref(&self) -> &[T] {
        self.chunk
    }
}

impl<'a, const CHUNK_LEN: usize, const CHUNKS: usize, T> Drop
    for ChunkReader<'a, CHUNK_LEN, CHUNKS, T>
where
    T: Copy + 'static,
{
    fn drop(&mut self) {
        // Mark the chunk as writable again.
        self.consumer.mark_chunk_writable(self.idx);
    }
}

pub fn create_channel<const CHUNK_LEN: usize, const CHUNKS: usize, T>() -> (
    Producer<CHUNK_LEN, CHUNKS, T>,
    Consumer<CHUNK_LEN, CHUNKS, T>,
)
where
    T: Copy + 'static,
{
    // In order to create the producer and consumer, we obviously need our inner state. We also need to make sure that
    // it's impossible for anyone else to access that inner state, which means taking a reference to the inner state and
    // creating the producer/consumer against that would not work, since it implies it could be done multiple times.
    //
    // So with that, we have to create it here to ensure it's not reusable. That brings us to our second problem: where
    // does the state live such that we can take a reference to it and avoid needing to `Arc<T>` it, etc? Well, so,
    // we're going to leak it.
    //
    // "I'm tired of leaking objects to get `'static` references being the go-to pattern... we shouldn't have a way to
    // create memory leaks!" you say, and you're right. By relying on the semantics of how the channel is used, we can
    // actually unleak the inner state, despite creating `'static` references to it, and do so in a logically consistent
    // way!
    //
    // As we know that there will only be one producer and one consumer attached to any given state object, we only have
    // two places that need to coordinate around when it's safe to unleak/drop the memory. Additionally, and thankfully,
    // we have another invariant on our side: we can generate references tied to the lifetime of the producer and consumer, breaking the link to the lifetime
    //   of the state itself
    //
    // Essentially, when a producer is dropped, we mark the inner state to indicate as much. This implies the producer
    // is fully done, and that there are no outstanding chunk writes happening, based on how we generate the chunk writer guard
    // object. With that, the consumer can poll for this state to figure out when it's safe to stop processing.. as if
    // there's no more readable chunks, and the producer has dropped, there's no reason to keep trying to consume from
    // the channel. When this happens, we can also safely unleak the inner state and correctly drop it.
    //
    // This assumes that the consumer task will be around as long or longer than producer tasks, but bugs happen and the
    // consumer task could also potentially crash. This would be bad from a cleanup standup, but also from a deadlock
    // standpoint, as well. As such, we also provide a mechanism for the producer to understand when the consumer has
    // dropped, which lets it flip to a fail open state. When this occurs, all attempts to acquire a chunk will fail.
    // This can be used as a signal by whatever owns the producer to stop trying to produce, and potentially to drop the
    // producer itself, which would then allow cleanup to also happen: since we have to know that the consumer has
    // dropped to know its time to fail open, we can also do the cleanup logic when the producer drops.

    let inner = Box::leak(Box::new(Inner::<CHUNK_LEN, CHUNKS, T>::new()));
    (Producer { inner }, Consumer { inner })
}
