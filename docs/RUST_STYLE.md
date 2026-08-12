# Rust Style Guide for Vector

> **Note:
** This is a draft document primarily intended for AI agents (like Claude) to understand Vector's Rust coding conventions. These guidelines help ensure consistent code generation and modifications.

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

### Component Feature Flags

New components (sources, sinks, transforms) must be behind feature flags:

```bash
# Build only specific component for faster iteration
cargo test --lib --no-default-features --features sinks-console sinks::console
```

See `features` section in `Cargo.toml` for examples.

### Cargo Dependency Feature Placement

[Cargo features are additive](https://doc.rust-lang.org/cargo/reference/features.html#feature-unification): once any crate in the dependency graph enables a feature, it is enabled for the entire build. This can be surprising — enabling a feature in one crate silently turns it on everywhere.

Always set `default-features = false` in `[workspace.dependencies]`. For feature declarations, the rule is simple:

- If only one crate needs a feature, declare it in that crate's own `Cargo.toml`.
- If multiple crates need it, declare it in `[workspace.dependencies]`.

Add a short comment near the dependency when the feature setup is non-obvious.

**Auditing:** when unsure whether a feature is truly needed or only transitively enabled, verify with:

```bash
# Show the full feature tree for the workspace
cargo tree -e features

# Narrow down to a specific dependency
cargo tree -e features -i <crate-name>
```

The `-i` flag shows which crates depend on `<crate-name>` and which features they activate, useful for tracing where a feature is coming from. If a feature only appears because another crate enables it, and your crate relies on that feature being present, declare it explicitly — otherwise your crate silently breaks if the other crate ever stops enabling it.
