# Rust Style Guide for Vector

This document outlines Rust coding conventions and patterns for Vector development.

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
- Use `rustfmt` to automatically organize them: `make fmt`

## Logging Style

Always use the [Tracing crate](https://tracing.rs/tracing/)'s key/value style:

**Correct:**

```rust
warn!(message = "Failed to merge value.", %error);
info!(message = "Processing batch.", batch_size, internal_log_rate_secs = 1);
```

**Incorrect:**

```rust
warn!("Failed to merge value: {}.", err);  // Don't do this
```

**Rules:**

- Events should be capitalized and end with a period
- Use `error` (not `e` or `err`) for error values
- Prefer Display over Debug: `%error` not `?error`
- Key/value pairs provide structured logging

## String Formatting

Prefer inline variable syntax in format strings (Rust 1.58+).

**Correct:**

```rust
format!("Error: {err}");
println!("Processing {count} items");
```

**Incorrect:**

```rust
format!("Error: {}", err);      // Unnecessary positional argument
println!("Processing {} items", count);
```

**Why:** Inline syntax is more readable and reduces mistakes with argument ordering.

## Panics

Code in Vector should **NOT** panic under normal circumstances.

- Panics are only acceptable when assumptions about internal state are violated (indicating a bug)
- All potential panics **MUST** be documented in function documentation
- Prefer `Result<T, E>` and proper error handling

## Feature Flags

New components (sources, sinks, transforms) must be behind feature flags:

```bash
# Build only specific component for faster iteration
cargo test --lib --no-default-features --features sinks-console sinks::console
```

See `features` section in `Cargo.toml` for examples.

## Common Patterns

### Development Tools

Vector uses `cargo vdev` for most development tasks. This is a custom CLI tool that wraps common operations:

```bash
cargo vdev check rust         # Clippy
cargo vdev check fmt          # Formatting check
cargo vdev check events       # Event instrumentation check
cargo vdev check licenses     # License compliance
cargo vdev test               # Unit tests
cargo vdev int test <name>    # Integration tests
cargo vdev fmt                # Format code
```

### Pre-Push Hook (Optional but Recommended)

Create `.git/hooks/pre-push` with:

```bash
#!/bin/sh
set -e

echo "Format code"
make fmt

echo "Running pre-push checks..."
make check-licenses
make check-fmt
make check-clippy
make check-markdown
make check-component-docs

./scripts/check_changelog_fragments.sh
```

Then: `chmod +x .git/hooks/pre-push`