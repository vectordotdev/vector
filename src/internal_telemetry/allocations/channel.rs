use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const WRITABLE: bool = false;
const READABLE: bool = true;

/// A fixed-size data chunk.
///
/// This is specifically aligned to 64 bytes, and uses a fixed-size layout, to optimize for performance.
#[repr(C, align(64))]
pub struct Chunk<const CHUNK_SIZE: usize, T: Copy>([MaybeUninit<UnsafeCell<T>>; CHUNK_SIZE]);

impl<const CHUNK_SIZE: usize, T: Copy> Chunk<CHUNK_SIZE, T> {
    /// Gets a mutable reference to the underlying data.
    ///
    /// ## Safety
    ///
    /// The caller is responsible for ensuring that they have exclusive access to this chunk, as the mutable reference
    /// is created in a way that circumvents normal aliasing rules.
    #[inline]
    unsafe fn as_mut(&self) -> &mut [T] {
        slice::from_raw_parts_mut(self.0.as_mut_ptr() as *mut _, CHUNK_SIZE)
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
    /// by the producer. While chunks are always fully initialized when created, ensuring that a read is never
    /// uninitialized, this does not ensure that previously-written data can not be read again if it was not overwritten
    /// by the producer but the given length was larger than what was actually written by the producer.
    #[inline]
    unsafe fn as_bounded_slice(&self, len: usize) -> &[T] {
        slice::from_raw_parts(self.0.as_ptr() as *const _, len)
    }
}

pub struct Inner<const CHUNK_SIZE: usize, const NUM_BUFFERS: usize, T: Copy> {
    chunk_state: [AtomicBool; NUM_BUFFERS],
    consumer_chunk_data_len: [AtomicUsize; NUM_BUFFERS],
    chunks: [Chunk<CHUNK_SIZE, T>; NUM_BUFFERS],
}

impl<const CHUNK_SIZE: usize, const NUM_BUFFERS: usize, T: Copy> Inner<CHUNK_SIZE, NUM_BUFFERS, T> {
    #[inline]
    fn try_find_readable_chunk_pos(&self) -> Option<usize> {
        for idx in 0..NUM_BUFFERS {
            // SAFETY: We only get `idx` from iterating from 0 to `NUM_BUFFERS`, and `self.chunk_state` is a fixed-size
            // array of size `NUM_BUFFERS`, so we know the index will never be out-of-bounds.
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
        for idx in 0..NUM_BUFFERS {
            // SAFETY: We only get `idx` from iterating from 0 to `NUM_BUFFERS`, and `self.chunk_state` is a fixed-size
            // array of size `NUM_BUFFERS`, so we know the index will never be out-of-bounds.
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
    fn try_find_readable_chunk(&self) -> Option<ChunkReader<'_, CHUNK_SIZE, NUM_BUFFERS, T>> {
        self.try_find_readable_chunk_pos().map(|idx| {
            let data_len = self.consumer_chunk_data_len[idx].load(Ordering::Acquire);

            // Wrap the chunk in a guard and pass it back to the caller.
            ChunkReader {
                inner: self,
                idx,

                // SAFETY: We take a reference to the chunk data only if the chunk is readable, which
                // implies the chunk won't be accessed for writing (chunks are read XOR write). The arrangement of
                // `Inner` ensures that there is only ever one consumer attached to a given `Inner`, so we know that
                // we do not have to worry about another call to `try_find_readable_chunk` taking a
                // reference to this chunk.
                data: unsafe { self.chunks[idx].as_bounded_slice(data_len) },
            }
        })
    }

    /// Attempts to find a writable chunk.
    ///
    /// If a writable chunk is found, `Some(ChunkWriter)` is returned, which wraps write access to the chunk with a
    /// guard, `ChunkWriter`, that provides convenience methods for writing and sending the chunk, as well as ensuring
    /// the chunk is always returned to the consumer by the time the chunk writer is dropped.
    #[inline]
    fn try_find_writable_chunk(&self) -> Option<ChunkWriter<'_, CHUNK_SIZE, NUM_BUFFERS, T>> {
        self.try_find_writable_chunk_pos().map(|idx| {
            // Wrap the chunk in a guard and pass it back to the caller.
            ChunkWriter {
                inner: self,
                idx,
                written: 0,

                // SAFETY: We take a mutable reference to the chunk data only if the chunk is writable, which
                // implies the chunk won't be accessed for reading (chunks are read XOR write). The arrangement of
                // `Inner` ensures that there is only ever one producer attached to a given `Inner`, so we know that
                // we do not have to worry about another call to `try_find_writable_chunk` taking a mutable
                // reference to this chunk.
                data: unsafe { self.chunks[idx].as_mut() },
            }
        })
    }

    /// Marks a chunk as readable.
    ///
    /// This also updates the chunk length so that a valid slice reference can be created when the chunk is read by the consumer.
    #[inline]
    fn mark_chunk_readable(&self, idx: usize, len: usize) {
        self.consumer_chunk_data_len[idx].store(len, Ordering::Relaxed);
        self.chunk_state[idx].store(READABLE, Ordering::Release);
    }

    /// Marks a chunk as writable.
    #[inline]
    fn mark_chunk_writable(&self, idx: usize) {
        self.chunk_state[idx].store(WRITABLE, Ordering::Release);
    }
}

pub struct Producer<'a, const CHUNK_SIZE: usize, const NUM_BUFFERS: usize, T: Copy> {
    inner: &'a Inner<CHUNK_SIZE, NUM_BUFFERS, T>,
}

impl<'a, const CHUNK_SIZE: usize, const NUM_BUFFERS: usize, T: Copy>
    Producer<'a, CHUNK_SIZE, NUM_BUFFERS, T>
{
    /// Acquires a chunk for writing.
    ///
    /// This method will search indefinitely for the next available writable chunk, so consumers should ensure that they
    /// consume readable chunks as fast as reasonably possible to reduce producer waiting.
    ///
    /// The chunk is returned in a guard, `ChunkWriter`, which ensures mutable access to the chunk as well as ensuring
    /// the chunk is sent (if it wasn't already) when the guard is dropped.
    #[inline]
    pub fn acquire_chunk(&mut self) -> ChunkWriter<'a, CHUNK_SIZE, NUM_BUFFERS, T> {
        loop {
            if let Some(chunk_writer) = self.inner.try_find_writable_chunk() {
                return chunk_writer;
            }
        }
    }
}

/// RAII structure used to provide exclusive write access to a [`Chunk`].
///
/// When the guard is dropped, the chunk will be sent to the consumer if it was not already.
pub struct ChunkWriter<'a, const CHUNK_SIZE: usize, const NUM_BUFFERS: usize, T: Copy> {
    inner: &'a Inner<CHUNK_SIZE, NUM_BUFFERS, T>,
    data: &'a mut [T],
    idx: usize,
    written: usize,
}

impl<'a, const CHUNK_SIZE: usize, const NUM_BUFFERS: usize, T: Copy>
    ChunkWriter<'a, CHUNK_SIZE, NUM_BUFFERS, T>
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
        let available = CHUNK_SIZE - self.written;
        if available == 0 {
            return None;
        }

        let elements_to_write = available.min(data.as_ref().len());

        // Copy the data over.
        let (_, remaining) = self.data.split_at_mut(available);
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
        self.inner.mark_chunk_readable(self.idx, self.written);
    }
}

impl<'a, const CHUNK_SIZE: usize, const NUM_BUFFERS: usize, T: Copy> Drop
    for ChunkWriter<'a, CHUNK_SIZE, NUM_BUFFERS, T>
{
    fn drop(&mut self) {
        self.send_inner();
    }
}

pub struct Consumer<'a, const CHUNK_SIZE: usize, const NUM_BUFFERS: usize, T: Copy> {
    inner: &'a Inner<CHUNK_SIZE, NUM_BUFFERS, T>,
}

impl<'a, const CHUNK_SIZE: usize, const NUM_BUFFERS: usize, T: Copy>
    Consumer<'a, CHUNK_SIZE, NUM_BUFFERS, T>
{
    /// Attempts to consume data from the channel.
    ///
    /// If a readable chunk is found, the given closure will be called with a reference to the chunk data. After the
    /// closure returns, the chunk will be released back for use by the producer, and `Some(O)` will be returned,
    /// containing the result of the closure.
    ///
    /// Otherwise, `None` is returned, indicating that no readable chunk was found.
    pub fn try_consume<F, O>(&self, mut f: F) -> Option<O>
    where
        F: FnMut(&[T]) -> O,
    {
        self.inner
            .try_find_readable_chunk()
            .map(|chunk| f(chunk.as_ref()))
    }
}

/// RAII structure used to provide exclusive read access to a [`Chunk`].
///
/// When the guard is dropped, the chunk will be sent back to the producer.
pub struct ChunkReader<'a, const CHUNK_SIZE: usize, const NUM_BUFFERS: usize, T: Copy> {
    inner: &'a Inner<CHUNK_SIZE, NUM_BUFFERS, T>,
    data: &'a [T],
    idx: usize,
}

impl<'a, const CHUNK_SIZE: usize, const NUM_BUFFERS: usize, T: Copy> AsRef<[T]>
    for ChunkReader<'a, CHUNK_SIZE, NUM_BUFFERS, T>
{
    fn as_ref(&self) -> &[T] {
        self.data
    }
}
impl<'a, const CHUNK_SIZE: usize, const NUM_BUFFERS: usize, T: Copy> Drop
    for ChunkReader<'a, CHUNK_SIZE, NUM_BUFFERS, T>
{
    fn drop(&mut self) {
        // Mark the chunk as writable again.
        self.inner.mark_chunk_writable(self.idx);
    }
}
