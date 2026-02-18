# Quick Reference for Vector Development

This guide provides quick commands and coding conventions for Vector development. It's designed to help both AI assistants and human
contributors get started quickly.

**For comprehensive information, see [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/DEVELOPING.md](docs/DEVELOPING.md).**

## Project Summary

Vector is a high-performance, end-to-end observability data pipeline written in Rust. It collects, transforms, and routes logs, metrics, and
traces from various sources to any destination. Vector is designed to be reliable, fast, and vendor-neutral, enabling dramatic cost
reduction and improved data quality for observability infrastructure.

## Project Structure

### Core Directories

- `/src/` - Main Rust source code
  - `sources/` - Data ingestion components
  - `transforms/` - Data processing and routing components
  - `sinks/` - Data output destinations
  - `config/` - Configuration system and validation
  - `topology/` - Component graph management
  - `api/` - GraphQL API for management and monitoring
  - `cli.rs` - Command-line interface

- `/lib/` - Modular library crates
  - `vector-lib/` - Unified library re-exporting core Vector components
  - `vector-core/` - Core event system and abstractions
  - `vector-config/` - Configuration framework with schema generation
  - `vector-buffers/` - Buffering and backpressure management
  - `codecs/` - Data encoding/decoding (JSON, Avro, Protobuf)
  - `enrichment/` - Data enrichment (GeoIP, custom tables)
  - `file-source/` - File watching and reading
  - `prometheus-parser/` - Prometheus metrics parsing

- `/config/` - Configuration examples and templates
- `/distribution/` - Packaging and deployment configs
  - `docker/` - Docker images (Alpine, Debian, Distroless)
  - `kubernetes/` - Kubernetes manifests
  - `systemd/` - SystemD service files
  - `debian/`, `rpm/` - Linux package configurations

- `/scripts/` - Build, test, and deployment automation
- `/docs/` - Developer documentation
- `/tests/` - Integration and E2E tests

## Two Different Workflows

### Rust Development (Most Common)

If you're working on Vector's Rust codebase (sources, sinks, transforms, core functionality):

**Format your code:**

```bash
make fmt
```

**Check formatting:**

```bash
make check-fmt
```

**Run Clippy (linter):**

```bash
make check-clippy
```

**Auto-fix Clippy issues:**

```bash
make clippy-fix
```

**Run unit tests:**

```bash
make test
# or
cargo nextest run --workspace --no-default-features --features "${FEATURES}"
```

**Run integration tests:**

```bash
# See available integration tests:
# cargo vdev int show
./scripts/run-integration-test.sh <integration-name>
```

See [Integration Tests](#integration-tests) section below for more details.

**Before committing (recommended checks):**

```bash
make fmt                      # Format code
make check-fmt                # Verify formatting
make check-clippy             # Run Clippy linter
make check-component-docs     # Check component documentation
./scripts/check_changelog_fragments.sh  # Verify changelog
```

### Website/Docs Development (Separate Process)

If you're working on vector.dev website or documentation content:

**Prerequisites:**

- Hugo static site generator
- CUE CLI tool
- Node.js and Yarn
- htmltest

**Run the site locally:**

```bash
cd website
make serve
# Navigate to http://localhost:1313
```

**Build website:**

```bash
cd website
make cue-build
```

**Note:** Website changes use Hugo, CUE, Tailwind CSS, and TypeScript. See [website/README.md](website/README.md) for details.

## Rust Coding Conventions

### Import Statements (`use`)

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

### Logging Style

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

### String Formatting

Prefer inline variable syntax in format strings (Rust 1.58+).

**Correct:**

```rust
format!("Error: {err}");
println!("Processing {count} items");
error!("Failed to connect: {error}");
```

**Incorrect:**

```rust
format!("Error: {}", err);      // Unnecessary positional argument
println!("Processing {} items", count);
error!("Failed to connect: {}", error);
```

**Why:** Inline syntax is more readable and reduces mistakes with argument ordering.

### Panics

Code in Vector should **NOT** panic under normal circumstances.

- Panics are only acceptable when assumptions about internal state are violated (indicating a bug)
- All potential panics **MUST** be documented in function documentation
- Prefer `Result<T, E>` and proper error handling

### Feature Flags

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
make check-component-docs

./scripts/check_changelog_fragments.sh
```

Then: `chmod +x .git/hooks/pre-push`

### Container Development

Vector supports development in Docker/Podman containers:

```bash
ENVIRONMENT=true make <target>
# Example: ENVIRONMENT=true make test
```

## Architecture Notes

### Component Development

- **Sources**: Ingest data from external systems
- **Transforms**: Modify, filter, or enrich event data
- **Sinks**: Send data to external systems

Component docs are auto-generated from code annotations. Run `make check-component-docs` after changes.

### Integration Tests

Integration tests verify Vector works with real external services. Require Docker or Podman.

**Run integration tests:**

```bash
# List available tests
cargo vdev int show

# Run specific test (example: aws)
cargo vdev int start aws # need to initiate dev environment first
cargo vdev int test aws
```

See [docs/DEVELOPING.md](docs/DEVELOPING.md#integration-tests) for adding new integration tests.

### Key Files

- `Makefile` - Common build/test/check targets
- `vdev/` - Custom development CLI tool
- `src/` - Rust source code
- `website/` - Hugo-based documentation site
- `tests/` - Integration and behavior tests

## Common Issues

### Formatting Fails

Run `make fmt` before committing. Formatting must be exact.

### Clippy Errors

Run `make clippy-fix` to auto-fix many issues. Manual fixes may be required.

### Component Docs Out of Sync

Component documentation is generated from code. Run:

```bash
make check-component-docs
```

### License Check Fails

After adding/updating dependencies:

```bash
cargo install dd-rust-license-tool --locked
make build-licenses
```

## Reference Documentation

These documents provide context that AI agents and developers need when working on Vector code.

### Essential for Code Changes

- **[STYLE.md](STYLE.md)** - Code style rules (formatting, const strings, code organization)
- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** - System architecture (sources, transforms, sinks, topology)
- **[docs/DEVELOPING.md](docs/DEVELOPING.md)** - Development workflow and testing

### Component Development

- **[docs/specs/component.md](docs/specs/component.md)** - Component specification (naming, configuration, health checks)
- **[docs/specs/instrumentation.md](docs/specs/instrumentation.md)** - Instrumentation requirements (event/metric naming)
- **[src/internal_events](src/internal_events)** - Internal event examples for telemetry

### Adding Documentation

- **[docs/DOCUMENTING.md](docs/DOCUMENTING.md)** - How to document code changes
- **[changelog.d/README.md](changelog.d/README.md)** - Adding changelog entries

### Full Guides

- **[CONTRIBUTING.md](CONTRIBUTING.md)** - Complete contributing guide
- **[website/README.md](website/README.md)** - Website development only (separate from Rust code)
