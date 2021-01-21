extern crate vector_global_alloc;

// Should conflict with the declaration from the `vector_global_alloc`.
#[global_allocator]
static ALLOC: std::alloc::System = std::alloc::System;

fn main() {}
