# Quick Reference for Vector Development

This guide provides quick commands and coding conventions for Vector development.

For comprehensive information, see [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/DEVELOPING.md](docs/DEVELOPING.md).

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
2. Run the appropriate Clippy command described under Rust Development below.
3. Fix any issues found (use `make clippy-fix` for auto-fixes)
4. Continue to next task or mark current task complete

Run this cycle after any code modification.

### Final validation step

After the task is complete run the following `make` commands to check for errors in tests and other
targets.

1. Run `make fmt` to format your code.
2. Run the narrowest relevant tests using the minimum feature set as described below.

## Code change workflows and validation

### Rust Development (Most Common)

If you're working on Vector's Rust codebase:

When building and running Vector with a configuration file, use `cargo vdev run <config>`. It
automatically selects the minimum set of features required by the configuration, reducing compile
times.

If `cargo vdev run <config>` fails, fall back to `cargo run -- --config <config>`.

Run `FEATURES="<features>" make check-clippy` to narrow down the feature list and disable default
features. If a representative configuration exists, derive its features with
`cargo vdev features <config>`. Do not infer features from file names; use `make check-clippy`
without `FEATURES` for full-feature validation.

#### Running tests

For most Rust changes, specify the relevant component feature directly:

```bash
make test FEATURES="sources-file" SCOPE="truncate"
```

If you have a representative configuration file, derive its required features automatically:

```bash
cargo vdev test --config path/to/config.yaml test_some_function
```

Other testing methods, from targeted to broad:

```bash
# Use a nextest filter expression (note the quoting)
make test SCOPE="-E 'test(foo) and not test(bar)'"

# Run all tests
make test
```

#### Running integration tests

```bash
# See available integration tests:
cargo vdev int show

# Run a specific integration test
cargo vdev int run <integration-name>
```

See [Integration Tests](#integration-tests) section below for more details.

#### If editing any markdown files

```bash
make check-markdown
```

#### If changing any user facing documentation, including examples, component configuration or VRL functions

```bash
make generate-docs
```

#### If modifying any external dependencies

Requires `dd-rust-license-tool`

```bash
make build-licenses
```


#### Before committing (recommended checks)

```bash
make fmt                      # Format code
make check-fmt                # Verify formatting
make check-clippy             # Run Clippy linter
make check-markdown           # Check markdown files
make check-generated-docs     # Check generated documentation
make check-changelog-fragments  # Verify changelog
```

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
make check-changelog-fragments
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

## Git Conventions

- **Commit messages:** Do NOT include co-authoring information from coding agents (i.e. avoid "Co-Authored-By: Claude" attribution)
- **Pull requests:** Do NOT add "Generated with Claude Code" or similar footers — keep PR descriptions focused on the technical changes

### Preserve Open Pull Request History

Before rewriting a branch that has been pushed, use `gh` when available to check whether the branch has an open pull request:

```bash
gh pr list --head "$(git branch --show-current)" --state open --json number,url
```

If `gh` is unavailable or the check fails, assume an open pull request exists.

When an open pull request exists, never rewrite published commits or force-push the branch. Push additional commits normally to preserve incremental review.

## Creating Pull Requests

Before opening a PR, read [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) and use it as the reference for the PR body structure and title.
