//! This crate is intended for sharing global allocator preferences and choices
//! across various dependent targets (vector itself, standalone benches).
//!
//! # Usage
//!
//! 1. Add to the dependencies.
//! 2. Add `extern crate vector_global_alloc` to your crate's main entrypoint (`src/lib.rs` or `src/main.rs`).
//! 3. Optionally tweak the features passed to this crate as a dependencly to pick the allocator.

#![deny(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations
)]

cfg_if::cfg_if! {
    if #[cfg(any(
        // When system allocator is forced...
        feature = "force-system",
        // Or when we want to use it as a platform default.
        target_os = "windows"
    ))] {
        // Use system allocator.
        // Ensure we're setting a global allocator either way to "grab the spot"
        // and prevent it from being defined anywhere else.
        #[global_allocator]
        static ALLOC: std::alloc::System = std::alloc::System;
    } else if #[cfg(any(
        // When jemallocator is forced...
        feature = "force-jemallocator",
        // Or when we want to use it as a platform default.
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd"
    ))] {
        // Use jemalloc as global allocator.
        #[global_allocator]
        static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;
    } else {
        // No allocator was picked, fail fast to alert the user about it.
        compile_error!(concat!(
            "No feature forcing a particular allocator was passed, and we didn't default to something meaningful.\n",
            "You have to excplicitly pass a feature to force a particulr allocator on this system.",
        ));
    }
}
