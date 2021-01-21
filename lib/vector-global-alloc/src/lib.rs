//! This crate is intended for sharing global allocator preferences and choices
//! across various dependent crates.
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
    if #[cfg(feature = "force-system")] {
        // Ensure we're setting a global allocator either way to "grab the spot" and
        // prevent it from being used anywhere else if no other allocator is set.
        #[global_allocator]
        static ALLOC: std::alloc::System = std::alloc::System;
    } else if #[cfg(feature = "jemallocator")] {
        // Use jemalloc as global allocator.
        #[global_allocator]
        static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;
    } else {
        // No allocator was picked, fail fast to alert the user about it.
        // `jemallocator` is supposed to be in the `default` featureset.
        compile_error!("Either \"force-system\" or \"jemallocator\" feature has to be enabled");
    }
}
