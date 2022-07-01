// TODO: can we speed up/remove the `is_initialized` check if instead we use `Cell<usize>` to track the highest idx that
// we've initialized for so far, and store that on the page table, since we have mutable access to it by nature of it
// being in thread-local storage, and then we get to avoid having to do an atomic load every single `get`?

// TODO: we know that our group ID will never be zero, which means we're just wasting a page for no good reason. we
// should write a lil helper method that takes a group ID and gives back the usize version, but shifted down by 1, and
// just make sure to use that for all group ID -> usize translations.

use std::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    sync::atomic::{AtomicBool, Ordering},
};

const POINTER_WIDTH: u32 = usize::BITS;
const PAGE_COUNT: usize = (POINTER_WIDTH - 1) as usize;

/// A lazily-allocated memory page.
///
/// Pages can be created in a const fashion and then allocate the necessary storage after the fact. This makes it
/// amenable to allocating the top-level pages for a storage scheme as part of a fixed-size scheme.
#[derive(Debug)]
struct LazilyAllocatedPage<T> {
    page_size: usize,
    initialized: AtomicBool,
    initialized_fast: UnsafeCell<bool>,
    data: UnsafeCell<MaybeUninit<Box<[T]>>>,
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
            initialized: AtomicBool::new(false),
            initialized_fast: UnsafeCell::new(false),
            data: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /*
    /// Gets whether or not this page has been initialized yet.
    fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }
    */

    /// Gets whether or not this page has been initialized yet, but quickly.
    fn is_initialized_fast(&self) -> bool {
        unsafe { self.initialized_fast.get().read() }
    }

    /// Initializes the page, allocating the underlying storage.
    fn initialize(&self) {
        if !self.initialized.load(Ordering::Acquire) {
            // Allocate the underlying storage for this page.
            let mut data = Vec::with_capacity(self.page_size);
            data.resize_with(self.page_size, T::default);

            // SAFETY: `LazilyAllocatedPage<T>::initialize` is only called by `PageTable<T>::get`, and `PageTable<T>` is
            // only stored/used in a thread-local fashion. This means that any access to `PageTable<T>::get` is
            // happening from the same thread that owns it, ensuring that the caller has exclusive access.
            //
            // Our usage of `self.initialized` ensures that we can't mistakenly try to initialize the same page again,
            // but its use is primarily for the synchronization necessary to support concurrent reads via `as_slice`,
            // not to synchronize mutable access in this method itself.
            unsafe { (&mut *self.data.get()).write(data.into_boxed_slice()) };

            unsafe {
                self.initialized_fast.get().write(true);
            }
            self.initialized.store(true, Ordering::Release);
        }
    }

    /// Gets a reference to the given element.
    ///
    /// # Safety
    ///
    /// It is the callers responsibility to ensure that they have a valid index for this page. Using a value that
    /// exceeds `self.page_size` will result in instant UB at best, and a process abort at worst.
    unsafe fn get_unchecked(&self, index: usize) -> &T {
        (&*self.data.get()).assume_init_ref().get_unchecked(index)
    }

    /*
    /// Gets a reference to all elements in the page.
    ///
    /// If the page has not yet been initialized (via `initialize`), then an empty slice is returned,
    fn as_slice(&self) -> &[T] {
        if self.state.load(Ordering::Relaxed) == INITIALIZED {
            // SAFETY: We know that if `self.state` is `INITIALIZED`, then `self.slots` is initialized.
            unsafe { (&*self.data.get()).assume_init_ref() }
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
        if *self.initialized.get_mut() {
            // SAFETY: We know that if `self.initialized` is `true`, then `self.data` is initialized.
            unsafe { (&mut *self.data.get()).assume_init_drop() }
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
    /// Gets a reference to the element at the given index.
    ///
    /// # Safety
    ///
    /// The caller must ensure that this method is not called concurrently.
    pub(crate) unsafe fn get(&self, idx: usize) -> &T {
        let (page_idx, page_subidx) = idx_to_page_idxs(idx);
        let page = self.pages.get_unchecked(page_idx);
        if !page.is_initialized_fast() {
            page.initialize();
        }
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
