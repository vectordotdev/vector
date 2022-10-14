use std::marker::PhantomData;

// `PhantomNotSend` respectfully copied from tokio-rs/tracing, as it's a damn useful snippet.
//
// Copyright (c) 2019 Tokio Contributors
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:

// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

/// `PhantomNotSend` is designed to do one simple thing: make a struct unable to be sent across
/// threads, aka `!Send`.  Normal Rust code cannot implement negatrve trait bounds like the standard
/// library can, but there's one simple trick that doctors hate: stuffing your struct with a
/// pointer.
///
/// Pointers are `!Send` and `!Sync` by default, so adding one to a struct also makes that struct
/// `!Send`/`!Sync`.  We don't have an actual pointer, we just fake it with `PhantomData`. 👻
///
/// `AllocationGuard` cannot be allowed to be sent across threads, as doing so would violate the
/// invariant that we drop our allocation group from the active allocation group TLS variable when
/// the guard drops or is exited.  If we did not enforce this, we would trash the thread-local data,
/// and allocations would not be associated correctly.
///
/// Specifically, though, we're fine with `AllocationGuard` being `Sync`, as it has no inherent
/// methods that can be used in such a way.  We implement `Sync` for `PhantomNotSend` as it has no
/// API anyways.
#[derive(Debug)]
pub(crate) struct PhantomNotSend {
    ghost: PhantomData<*mut ()>,
}

impl PhantomNotSend {
    pub(crate) const fn default() -> Self {
        Self { ghost: PhantomData }
    }
}

/// # Safety
///
/// Trivially safe, as `PhantomNotSend` doesn't have any API.
unsafe impl Sync for PhantomNotSend {}
