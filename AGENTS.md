# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Vector is a high-performance, end-to-end observability data pipeline written in Rust. It's maintained by Datadog's Community Open Source Engineering team. Vector collects, transforms, and routes logs, metrics, and traces to any vendor.

## Repository Location

This repository lives under the `vectordotdev` GitHub organization and is locally symlinked to `~/vdd/vector`.

## Development Commands

### Building

```bash
# Quick compile check (fast)
cargo check

# Development build
cargo build
make build-dev

# Release build
cargo build --release
make build

# Build with all features
cargo build --all-features

# Build specific component only (faster)
cargo build --no-default-features --features sinks-console
```

### Testing

```bash
# Run unit tests (requires cargo-nextest: https://nexte.st/)
cargo nextest run
make test

# Run specific test
cargo nextest run <test_name>
make test SCOPE="sources::example"

# Run integration tests (requires Docker/Podman)
make test-integration
make test-integration-<name>  # e.g., test-integration-kafka

# Run behavioral tests
make test-behavior

# Run all tests
make test-all
```

### Using vdev CLI

The `vdev` CLI tool is Vector's custom development tool:

```bash
# Install vdev
cargo install --path vdev

# Run unit tests
cargo vdev test

# Run integration tests
cargo vdev int test <integration_name>  # e.g., aws, kafka
cargo vdev int show  # list available integration tests

# Run checks
cargo vdev check rust --fix  # Run clippy with fixes
cargo vdev check fmt         # Check formatting
cargo vdev check events      # Check internal metrics
cargo vdev check licenses    # Check LICENSE-3rdparty.csv
```

### Code Quality

```bash
# Format code
cargo fmt
make fmt

# Run clippy linter
cargo clippy --all-targets
make check-clippy
cargo vdev check rust --fix

# Check licenses
cargo vdev check licenses
make check-licenses

# Update licenses
make build-licenses  # requires dd-rust-license-tool

# Check component documentation
make check-component-docs

# Run all checks
make check-all  # (slow, runs everything)
```

### Benchmarking

```bash
# Run benchmarks
cargo bench <benchmark_name>
make bench SCOPE="transforms::example"
```

### Docker/Podman Development Environment

Vector provides a Docker-based development environment:

```bash
# Enter development shell
make environment

# Run commands in environment
make test ENVIRONMENT=true
make build-dev ENVIRONMENT=true

# Clean environment
make environment-clean
```

## Architecture

### Directory Structure

- **`/src`** - Main Vector source code
  - **`/src/sources`** - Data input components (file, http, kafka, etc.)
  - **`/src/transforms`** - Data transformation components (remap, filter, aggregate, etc.)
  - **`/src/sinks`** - Data output components (elasticsearch, s3, datadog, etc.)
  - **`/src/config`** - Configuration parsing and validation
  - **`/src/internal_events`** - Internal telemetry/instrumentation
  - **`/src/api`** - GraphQL API for runtime introspection
  - **`/src/conditions`** - Conditional logic for routing/filtering

- **`/lib`** - Shared libraries isolated from main binary
  - **`/lib/vector-lib`** - Core Vector library used by all components
  - **`/lib/vector-core`** - Core event data structures and traits
  - **`/lib/vector-config`** - Configuration schema and validation
  - **`/lib/vector-vrl`** - Vector Remap Language (VRL) functions
  - **`/lib/codecs`** - Encoding/decoding for various formats
  - **`/lib/vector-buffers`** - Buffering implementations
  - **`/lib/file-source`** - File watching/tailing functionality

- **`/tests`** - High-level integration and E2E tests
- **`/benches`** - Performance benchmarks
- **`/vdev`** - Development CLI tool
- **`/scripts`** - Build and maintenance scripts
- **`/distribution`** - Distribution artifacts (Docker, packages, etc.)

### Component Architecture

Vector uses a pipeline architecture with three main component types:

1. **Sources** - Ingest data from external systems (files, networks, APIs)
2. **Transforms** - Modify, enrich, or filter events in-flight
3. **Sinks** - Send data to external systems (databases, cloud services, etc.)

Each component is feature-gated in `Cargo.toml` (e.g., `sinks-elasticsearch`, `sources-kafka`).

### VRL (Vector Remap Language)

VRL is Vector's domain-specific language for transforming observability data. VRL functions are defined in `lib/vector-vrl/functions` and the language itself is maintained in a separate repository (https://github.com/vectordotdev/vrl).

## Coding Standards

### Logging Style

Always use the `tracing` crate's key/value style:

```rust
// ❌ Don't do this
warn!("Failed to merge value: {}.", err);

// ✅ Do this
warn!(message = "Failed to merge value.", %error);
```

- Events must be capitalized and end with a period
- Always spell out `error`, never use `e` or `err`
- Prefer Display over Debug: `%error` not `?error`

### Feature Flags

All new components must be behind feature flags:

```bash
# Build only specific component (faster iteration)
cargo test --lib --no-default-features \
  --features sinks-console sinks::console
```

### Dependencies

- Dependencies should be carefully selected and avoided if possible
- Component-specific dependencies must be optional and gated by feature flags
- Review dependency additions carefully (see `/docs/REVIEWING.md`)

### Panics

Code should NOT panic except in rare cases where assumptions about state are violated due to clear bugs. All potential panics MUST be documented in function documentation.

### Code Formatting

- Use `rustfmt` exclusively - configuration in `.rustfmt.toml`
- Run `make fmt` or `cargo fmt` before committing
- Sometimes macros cannot be formatted by rustfmt - manual tweaking may be needed

### Const Strings

Use compile-time constants instead of raw string literals when re-using strings:

```rust
// ✅ Preferred
const FIELD_NAME: &str = "timestamp";
// ❌ Avoid repeating raw strings
```

## Testing Guidelines

### Test Organization

- **Unit tests** - Inline tests throughout code, no external services required
- **Integration tests** - Tests requiring external services (run in Docker)
- **Blackbox tests** - Vector test harness for performance/correctness
- **Property tests** - Use `proptest` crate for property-based testing

### Integration Test Requirements

When adding integration tests:
- Service must run in Docker container
- Use unique port configured via environment variable
- Add `test-integration-<name>` target to Makefile
- Update `.github/workflows/integration.yml` workflow

### Running Specific Components Tests

```bash
# Fast iteration with cargo-watch
cargo watch -s clear -s \
  'cargo test --lib --no-default-features \
   --features=transforms-reduce transforms::reduce'
```

## Pull Request Guidelines

### Commit Format

PR titles must follow [Conventional Commits](https://www.conventionalcommits.org):

```
feat(new sink): new `xyz` sink
feat(tcp source): add foo bar baz feature
fix(tcp source): fix foo bar baz bug
chore: improve build process
docs: fix typos
```

### Changelog

All PRs are assumed to include user-facing changes unless labeled `no-changelog`. Add changelog entries per `changelog.d/README.md`.

### CI/CD

- CI uses GitHub Actions (`.github/workflows/`)
- Tests run automatically unless PR has `ci-condition: skip` label
- Some long-running tests only run nightly

## Kubernetes Development

For Kubernetes-specific work:

### Requirements
- `tilt` - https://tilt.dev/
- `docker` or `podman`
- `kubectl`
- `minikube` or other k8s cluster

### Development Flow
```bash
# Start local cluster and auto-rebuild on changes
tilt up

# Run E2E tests
CONTAINER_IMAGE_REPO=<your-name>/vector-test make test-e2e-kubernetes

# Quick iteration (skip full build)
QUICK_BUILD=true USE_MINIKUBE_CACHE=true make test-e2e-kubernetes
```

### Kubernetes Code Location
- Source: `src/sources/kubernetes_logs`
- API client: `src/kubernetes`
- E2E framework: `lib/k8s-test-framework`
- E2E tests: `lib/k8s-e2e-tests`

## Performance Profiling

Vector includes benchmarking in `/benches`. For deeper profiling:

```bash
# Build with debug symbols
# Edit Cargo.toml: [profile.release] debug = true

# Run Vector
cargo run --release -- --config my_config.toml

# Profile with perf (Linux)
perf record -F99 --call-graph dwarf -p $VECTOR_PID <load_command>

# Generate flamegraph
perf script | inferno-collapse-perf > stacks.folded
cat stacks.folded | inferno-flamegraph > flamegraph.svg
```

## Minimum Supported Rust Version (MSRV)

MSRV is specified in `Cargo.toml` as `rust-version`. Currently there is no fixed MSRV policy - it can be bumped when needed for dependencies or language features.

## Internal Telemetry

Vector emits its own telemetry using:
- **Logging**: `tracing` crate with structured fields
- **Metrics**: Internal metrics system
- **Events**: Defined in `src/internal_events`

### Disabling Rate Limiting
```bash
# CLI flag
vector --config vector.yaml -r 1

# Environment variable
VECTOR_INTERNAL_LOG_RATE_LIMIT=1 vector --config vector.yaml

# Per-statement
warn!(message = "Error occurred.", %error, internal_log_rate_limit = false);
```

## Useful Tips

### Faster Builds with sccache

Install and configure [sccache](https://github.com/mozilla/sccache) to cache compilation artifacts across builds.

### Pre-push Hook

Create `.git/hooks/pre-push` to run checks before pushing:

```bash
#!/bin/sh
set -e
make fmt
make check-licenses
make check-fmt
make check-clippy
make check-component-docs
```

### Generating Sample Logs

```bash
# Install flog: https://github.com/mingrammer/flog
flog --bytes $((100 * 1024 * 1024)) > sample.log
```
