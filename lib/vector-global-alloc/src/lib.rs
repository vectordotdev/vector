//! This crate is intended for sharing global allocator preferences and choices
//! across various dependent targets (vector itself, standalone benches).
//!
//! # Usage
//!
//! 1. Add to the dependencies.
//! 2. Add `extern crate vector_global_alloc` to your crate's main entrypoint (`src/lib.rs` or `src/main.rs`).
//! 3. Tweak the features passed to this crate as a dependencly to pick the allocator.

#![deny(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations
)]

// We have to deal with linker-level issues related to the
cfg_if::cfg_if! {
    if #[cfg(feature = "system")] {
        // Use system allocator.
        // Ensure we're setting a global allocator either way to "grab the spot"
        // and prevent it from being defined anywhere else.
        #[global_allocator]
        static ALLOC: std::alloc::System = std::alloc::System;
    } else if #[cfg(feature = "jemalloc")] {
        // Use jemalloc as global allocator.
        #[global_allocator]
        static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;
    } else {
        // No allocator was picked, fail fast to alert the user about it.
        // If you want to test this code, use the `test.sh` at the crate root.
        // If you're usin this crate, and get this error, make sure you always
        // specify the allocator you want to use by passing a feature.
        compile_error!("You have to excplicitly pass a feature to force a particular allocator.");
    }
}
