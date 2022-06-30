use std::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    sync::atomic::{AtomicUsize, Ordering},
};

const POINTER_WIDTH: u32 = usize::BITS;
const PAGE_COUNT: usize = (POINTER_WIDTH + 1) as usize;

const UNINITIALIZED: usize = 0;
const INITIALIZING: usize = 1;
const INITIALIZED: usize = 2;

/// A lazily-allocated memory page.
///
/// Pages can be created in a const fashion and then allocate the necessary storage after the fact. This makes it
/// amenable to allocating the top-level pages for a storage scheme as part of a fixed-size scheme.
#[derive(Debug)]
struct LazilyAllocatedPage<T> {
    page_size: usize,
    state: AtomicUsize,
    slots: UnsafeCell<MaybeUninit<Box<[T]>>>,
}

impl<T> LazilyAllocatedPage<T>
where
    T: Default,
{
    /// Creates a new `LazilyAllocatedPage` in an uninitialized state.
    ///
    /// Callers must initialize the underlying storage by calling `initialize`, which will allocate enough storage to
    /// store `page_size` elements and initialize all elements to the default value of `T`.
    const fn new(page_size: usize) -> Self {
        Self {
            page_size,
            state: AtomicUsize::new(UNINITIALIZED),
            slots: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Gets whether or not this page has been initialized yet.
    fn is_initialized(&self) -> bool {
        self.state.load(Ordering::Acquire) == INITIALIZED
    }

    /// Initializes the page, allocating the necessary underlying storage.
    fn initialize(&self) {
        // Try to acquire the right to initialize this page, if it's uninitialized.
        //
        // If we lose the race, it's because another caller is currently initializing it, in which case we wait for them
        // to complete... or because it's already initialize.
        if self
            .state
            .compare_exchange(
                UNINITIALIZED,
                INITIALIZING,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            // Allocate the underlying storage for this page.
            let mut slots = Vec::with_capacity(self.page_size);
            slots.resize_with(self.page_size, T::default);

            // SAFETY: We have exclusive access to `self.slots` if we won the race to set `self.state` to
            // `INITIALIZING`. Callers could still concurrently call `get_unchecked`, but that method is unsafe
            // specifically because it's a violation of the API contract to not call `initialize` first before
            // `get_unchecked`.
            unsafe { (&mut *self.slots.get()).write(slots.into_boxed_slice()) };

            self.state.store(INITIALIZED, Ordering::Release);
        } else {
            // Another caller is initializing this page, so wait for them to finish before we return.
            while self.state.load(Ordering::Relaxed) != INITIALIZED {}
        }
    }

    /// Gets a reference to the given element.
    ///
    /// # Safety
    ///
    /// It is the callers responsibility to ensure that they have a valid index for this page. This is given by passing
    /// the regular group ID into `id_to_page`, where the page index and page subindex are given. A given page subindex
    /// is only valid for the page index it was given with.
    ///
    /// Using any other values are instant UB, and will likely cause the process to abort.
    unsafe fn get_unchecked(&self, index: usize) -> &T {
        (&*self.slots.get()).assume_init_ref().get_unchecked(index)
    }

    /*
    /// Gets a reference to all elements in the page.
    ///
    /// If the page has not yet been initialized (via `initialize`), then an empty slice is returned,
    fn as_slice(&self) -> &[T] {
        if self.state.load(Ordering::Relaxed) == INITIALIZED {
            // SAFETY: We know that if `self.state` is `INITIALIZED`, then `self.slots` is initialized.
            unsafe { (&*self.slots.get()).assume_init_ref() }
        } else {
            &[]
        }
    }*/
}

// SAFETY: Pages are safe to access concurrently across threads after initialization, and are safe to access
// concurrently prior to initialization so long as the access is not via `get_unchecked`, which has its own safety
// requirements for safe usage.
unsafe impl<T> Sync for LazilyAllocatedPage<T> where T: Sync {}

impl<T> Drop for LazilyAllocatedPage<T> {
    fn drop(&mut self) {
        if *self.state.get_mut() == INITIALIZED {
            // SAFETY: We know that if `self.state` is `INITIALIZED`, then `self.slots` is initialized.
            unsafe { (&mut *self.slots.get()).assume_init_drop() }
        }
    }
}

/// A lazily-allocated storage container with power-of-two expansion.
///
/// This data structure represents the prototypical page table, containing an array of memory pages and the logic to map
/// a logical address to a specific page and slot within that page.
pub(crate) struct PageTable<T> {
    pages: [LazilyAllocatedPage<T>; PAGE_COUNT],
}

impl<T> PageTable<T>
where
    T: Default,
{
    /// Visits all initialized indexes.
    /*pub fn visit<F>(&self, _f: F)
    where
        F: Fn(usize, &T),
    {
    }*/

    /// Registers the given index.
    ///
    /// This ensures that the necessary storage at the given index is allocated and initialized.
    pub fn register(&self, idx: usize) {
        let (page_idx, _) = idx_to_page_idxs(idx);

        // SAFETY: `page` can never be a value greater than `PAGE_COUNT`.
        let page = unsafe { self.pages.get_unchecked(page_idx) };
        if !page.is_initialized() {
            page.initialize();
        }
    }

    /// Gets a reference to the element at the given index.
    ///
    /// # Safety
    ///
    /// This function assumes that the page where the given index lives has been previously initialized via `register`.
    /// Otherwise, this call will trigger instant UB, and will likely cause the process to abort.
    pub unsafe fn get(&self, idx: usize) -> &T {
        let (page_idx, page_subidx) = idx_to_page_idxs(idx);
        let page = self.pages.get_unchecked(page_idx);
        page.get_unchecked(page_subidx)
    }
}

impl<T> Default for PageTable<T>
where
    T: Default,
{
    fn default() -> Self {
        let mut maybe_pages: [MaybeUninit<LazilyAllocatedPage<T>>; PAGE_COUNT] =
            unsafe { MaybeUninit::uninit().assume_init() };

        let mut page_idx: u32 = 0;
        for page in &mut maybe_pages[..] {
            let page_size = 2usize.pow(page_idx);
            page.write(LazilyAllocatedPage::new(page_size));
            page_idx += 1;
        }

        let pages = unsafe { mem::transmute::<_, _>(maybe_pages) };

        Self { pages }
    }
}

#[inline]
const fn idx_to_page_idxs(idx: usize) -> (usize, usize) {
    let page_idx = POINTER_WIDTH - idx.leading_zeros();
    let page_size = 1 << page_idx.saturating_sub(1);
    let page_subidx = idx ^ page_size;

    // SAFETY: We can blindly cast to `usize` as both `POINTER_WIDTH` and `leading_zeros` will only ever return values
    // that track the number of bits in a pointer, and it is impossible for `usize` to not be able to hold a number
    // describing its own bit length.
    (page_idx as usize, page_subidx)
}
