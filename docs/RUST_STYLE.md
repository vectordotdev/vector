# Rust Style Guide for Vector

For Vector-specific conventions, see the [Vector Style Guide](../STYLE.md).

## Formatting

Use `make fmt` to format, `make check-fmt` to check formatting, and `make check-clippy` to lint.
Manually format macro contents that `rustfmt` cannot handle.

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

## Panics

Code in Vector should **NOT** panic under normal circumstances.

- Panics are only acceptable when assumptions about internal state are violated (indicating a bug)
- All potential panics **MUST** be documented in function documentation
- Prefer `Result<T, E>` and proper error handling

## Synchronization

Prefer `std::sync` primitives such as `Mutex` and `RwLock` for synchronous code.

In async code, synchronous locks can block runtime threads and cause deadlocks. Use Tokio's
async primitives when needed; see the
[`tokio::sync::Mutex` documentation](https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html)
for guidance on choosing between synchronous and async mutexes.
