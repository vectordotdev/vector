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
  - `api/` - gRPC API for management and monitoring
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

## Development Workflow

### Iterative Development Process

When working on Vector's Rust codebase, follow this iterative development cycle:

1. Make code changes
2. Run `make check-clippy` to check for linting issues
3. Fix any issues found (use `make clippy-fix` for auto-fixes)
4. Continue to next task or mark current task complete

Run this cycle after any code modification.

When editing markdown files (*.md), run `make check-markdown` after changes.

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
make check-markdown           # Check markdown files
make check-generated-docs     # Check generated documentation
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

## Configuration Format

Always generate Vector configuration examples in **YAML** unless the user explicitly asks for TOML or JSON. YAML is Vector's recommended and default configuration format.

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
make check-generated-docs

./scripts/check_changelog_fragments.sh
```

Then: `chmod +x .git/hooks/pre-push`

## Detailed Documentation

| Topic | Document |
| ----- | -------- |
| Rust style patterns | [docs/RUST_STYLE.md](docs/RUST_STYLE.md) |
| Code style rules (formatting, const strings, organization) | [STYLE.md](STYLE.md) |
| System architecture (sources, transforms, sinks, topology) | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Component specification (naming, configuration, health checks) | [docs/specs/component.md](docs/specs/component.md) |
| Instrumentation requirements (event/metric naming) | [docs/specs/instrumentation.md](docs/specs/instrumentation.md) |
| How to document code changes | [docs/DOCUMENTING.md](docs/DOCUMENTING.md) |
| Adding changelog entries | [changelog.d/README.md](changelog.d/README.md) |

## Architecture Notes

### Component Development

- **Sources**: Ingest data from external systems
- **Transforms**: Modify, filter, or enrich event data
- **Sinks**: Send data to external systems

Component docs are auto-generated from code annotations. Run `make check-generated-docs` after changes.

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

### Generated Docs Out of Sync

Documentation is generated from code. Run:

```bash
make check-generated-docs
```

### License Check Fails

After adding/updating dependencies:

```bash
cargo install dd-rust-license-tool --locked
make build-licenses
```

## Creating Pull Requests

Before opening a PR, read [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) and use it as the reference for the PR body structure and title.

### PR Title Format

PR titles must follow the [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) spec and are validated by `.github/workflows/semantic.yml`.

Examples:

```text
feat(kafka source): add consumer group lag metric
fix(loki sink): handle empty label sets correctly
docs(internal docs): update contributing guide
chore(deps): bump tokio to X
```
