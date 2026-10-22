# Rust Style Guide for Vector

This guide covers general Rust coding practices used in Vector, including conventions that
automated tooling does not fully capture. For repository layout, telemetry, dependency policy,
component feature flags, and configuration design, see the [Vector Style Guide](../STYLE.md).

## Formatting

Format all Rust code with `rustfmt`. Vector's [`.rustfmt.toml`](../.rustfmt.toml) is applied
automatically when running `cargo fmt` from the repository. Use `make fmt` to apply repository
formatting and `make check-fmt` to check it without changing files. Use `make check-clippy`
to enforce the workspace Clippy lints configured in [`Cargo.toml`](../Cargo.toml).

For installation instructions, see the
[rustfmt documentation](https://github.com/rust-lang/rustfmt#on-the-stable-toolchain).
`rustfmt` cannot always format code inside macros; adjust such code manually when needed.

## Import Statements (`use`)

All `use` statements must be at the **top of the file/module** or at the top of `mod tests`.
This is for consistency.

**Correct:**

```rust
use std::time::Duration;
use governor::clock;
use crate::config::TransformConfig;

fn my_function() {
    // function code
}
```

**Incorrect:**

```rust
fn my_function() {
    use std::time::Duration;  // WRONG; Do not insert `use` inside functions
    // function code
}
```

**Organization:**

- Group imports: `std` → external crates → internal (`crate::`)

## Unit tests

Unit tests for a type live in the same source file as that type (`#[cfg(test)] mod tests` at
the bottom of the file) unless they exercise behavior that spans multiple modules. Cross-module
tests belong with the composing type, not in a catch-all tests file for the crate.

## Const strings

When the same non-dynamic string literal is used more than once, prefer a
[compile-time constant](https://doc.rust-lang.org/std/keyword.const.html) when reasonable.
This reduces the risk of typos, especially for similar names such as event metadata fields.
Take the opportunity to replace repeated raw string literals with constants when modifying
existing code.

## Panics

Code in Vector should **NOT** panic under normal circumstances.

- Panics are only acceptable when assumptions about internal state are violated (indicating a bug)
- All potential panics **MUST** be documented in function documentation
- Prefer `Result<T, E>` and proper error handling

## Synchronization

Synchronization can be a very common sight when writing multi-threaded code in any language, and
this document does not aim to familiarize you with all of the common synchronization primitives and
their intended usage. Instead, however, there are some caveats that you must be aware of when using
synchronization primitives in synchronous versus asynchronous code.

Generally speaking, developers will lean on the synchronization primitives in `std::sync`, such as
`Mutex`, `RwLock`, and so on. These are typically the right choice -- in terms of using the ones
provided by `std`, vs alternative implementations of the same primitives -- as they're well-tested,
and have been improving over time in terms of performance. However, developers must be careful when
using these primitives in asynchronous code, as their behavior can sometimes adversely affect the
performance and correctness of the program.

To wit, developers must exercise caution when using synchronous (i.e. `std::sync`, `parking_lot`,
etc) synchronization primitives in asynchronous code, as they can be used in a way that deadlocks
the asynchronous runtime even though the code compiles and appears to be correct. In some cases,
you'll need to use asynchronous synchronization primitives, namely the ones from `tokio`
itself. The documentation on `tokio`'s own
[`Mutex`](https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html), for example, calls out the
specifics of when and where you might need to use it vs the one from `std::sync`.
