# Vector - Project Overview for AI coding assistant

## Project Summary
Vector is a high-performance, end-to-end observability data pipeline written in Rust. It collects, transforms, and routes logs, metrics, and traces from various sources to any destination. Vector is designed to be reliable, fast, and vendor-neutral, enabling dramatic cost reduction and improved data quality for observability infrastructure.

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

### Essential Commands

#### Environment Setup
```bash
# Use containerized development environment (recommended for new contributors)
export CONTAINER_TOOL="podman"  # Optional: default is docker
make environment               # Enter development shell

# Or setup native environment
cargo install -f --path vdev   # Install Vector development CLI
```

#### Building
```bash
# Development build
make build-dev
# or in container
make build-dev ENVIRONMENT=true

# Release build
make build

# Cross-compilation (various targets)
make build-x86_64-unknown-linux-gnu
make build-aarch64-unknown-linux-gnu
```

#### Testing
```bash
# Unit tests (using cargo-nextest)
make test

# Integration tests with real services
make test-integration

# Specific integration test
make test-integration-SCOPE="kafka"

# Kubernetes E2E tests
make test-e2e-kubernetes

# Behavioral tests (configuration, VRL)
make test-behavior

# Component validation
make test-component-validation
```

#### Development Tools
```bash
# Code quality checks
make check              # Format, clippy, docs
make check-fmt         # Format checking only
make check-clippy      # Linting only

# Format code
make fmt

# Generate documentation
make generate-component-docs
make build-rustdoc

# Performance testing
make bench
make bench-all
```

### Development Configuration
Vector uses YAML configuration files. Development configs can be placed in:
- `config/vector.yaml` - Main config file
- `config/examples/` - Example configurations

After building, run Vector with:
```bash
./target/release/vector --config config/vector.yaml
```

## Key Components

### Component Architecture
Vector follows a **directed acyclic graph (DAG)** model:
- **Sources** → ingest data from various inputs
- **Transforms** → process, route, and modify data
- **Sinks** → output data to destinations

#### Sources (40+ supported)
- **Files**: `file`, `stdin`, `journald`
- **Network**: `http_server`, `socket`, `syslog`
- **Cloud**: `aws_s3`, `aws_sqs`, `gcp_pubsub`, `kafka`
- **Metrics**: `host_metrics`, `prometheus`, `statsd`
- **Containers**: `docker_logs`, `kubernetes_logs`

#### Transforms (15+ available)
- **Routing**: `filter`, `route`, `sample`
- **Processing**: `remap` (VRL), `aggregate`, `dedupe`
- **Conversion**: `log_to_metric`, `metric_to_log`
- **Enrichment**: `aws_ec2_metadata`

#### Sinks (50+ supported)
- **Cloud Storage**: `aws_s3`, `azure_blob`, `gcp_cloud_storage`
- **Databases**: `clickhouse`, `postgres`, `elasticsearch`
- **Observability**: `datadog_logs`, `prometheus`, `splunk_hec`
- **Messaging**: `kafka`, `pulsar`, `redis`
- **Files**: `file`, `console`

### Configuration System
- **Format Support**: YAML, TOML, JSON
- **Schema Validation**: Automatic schema generation and validation
- **Environment Variables**: Full interpolation support
- **Hot Reloading**: Configuration changes without restart
- **Secret Management**: Integration with secret backends
- **Component Discovery**: Feature-flag based component inclusion

## Testing Strategy

### Test Types
Vector employs comprehensive multi-layered testing:

#### Unit Tests
- **Framework**: `cargo nextest` with parallel execution
- **Retries**: 3 retries for flaky tests
- **Timeout**: 30 seconds with graceful termination

#### Integration Tests (25+ services)
- **Cloud**: AWS, GCP, Azure services
- **Databases**: Elasticsearch, ClickHouse, MongoDB, PostgreSQL, Redis
- **Messaging**: Kafka, NATS, Pulsar, AMQP, MQTT
- **Observability**: Datadog, Prometheus, Loki, Splunk

#### End-to-End Tests
- **Real API Testing**: Datadog logs/metrics, OpenTelemetry
- **Kubernetes**: Multi-version K8s cluster testing (v1.19-v1.23)
- **Container Runtimes**: Docker, containerd

#### Performance Testing
- **Regression Detection**: Automated performance monitoring
- **Benchmarks**: Multiple benchmark suites (transforms, codecs, remap)
- **Continuous Monitoring**: Weekly performance regression checks

### Testing Infrastructure
- **Docker Compose**: Isolated service environments per integration
- **Custom K8s Framework**: Real Kubernetes cluster testing
- **Service Mesh**: Testing with actual external services

## Build System

### Build Tools
- **Primary**: `make` interface with environment detection
- **Rust**: Cargo with workspace management
- **Cross-compilation**: Docker-based cross-compilation
- **Containerization**: Multi-stage Docker builds

### Build Targets
- **Linux**: x86_64/aarch64 (GNU/musl), ARM variants
- **macOS**: x86_64/aarch64
- **Windows**: x86_64 (MSVC)

### Feature System
Vector uses extensive Cargo feature flags for modular compilation:
```toml
# Core features
default = ["api", "enrichment-tables", "sinks", "sources", "transforms"]

# Component-specific features  
sources = ["sources-logs", "sources-metrics"]
sinks = ["sinks-logs", "sinks-metrics"]

# Individual components
sources-kafka = ["dep:rdkafka"]
sinks-elasticsearch = ["transforms-metric_to_log"]
```

## CI/CD Pipeline

### GitHub Actions Workflows
- **Main Pipeline**: `.github/workflows/test.yml`
  - Code quality (rustfmt, clippy)
  - Unit tests and documentation
  - Component validation
  - License compliance

- **Integration Testing**: `.github/workflows/integration.yml`
  - Matrix testing across 25+ services
  - Docker-based service isolation

- **E2E Testing**: `.github/workflows/e2e.yml`
  - Real API testing (scheduled nightly)
  - Kubernetes multi-version testing

### Quality Gates
- **Code Formatting**: rustfmt enforcement
- **Linting**: Clippy with custom rules
- **Security**: cargo-deny for vulnerability scanning
- **License Compliance**: Approved license list enforcement
- **Performance**: Automated regression detection

## Platform Support
- **Linux**: Full support (all distributions)
- **macOS**: Full support (Intel and Apple Silicon)
- **Windows**: Full support (Server 2019+, Windows 10+)
- **Containers**: Docker, Kubernetes, extensive orchestration support
- **Cloud**: Native integrations for AWS, GCP, Azure

## Important Files

### Configuration
- `vector.yaml` - Main configuration file
- `config/examples/` - Example configurations
- `Cargo.toml` - Main package configuration with workspace
- `rust-toolchain.toml` - Rust version specification

### Build System
- `Makefile` - Primary build interface
- `Cross.toml` - Cross-compilation configuration
- `build.rs` - Build script for code generation

### CI/CD
- `.github/workflows/` - GitHub Actions workflows
- `scripts/` - Build and deployment automation
- `.config/nextest.toml` - Test runner configuration

### Documentation
- `README.md` - Project overview and quick start
- `docs/DEVELOPING.md` - Developer guide
- `docs/ARCHITECTURE.md` - Architecture documentation

## Security Considerations

### Secure Defaults
- Memory-safe Rust implementation
- No unsafe code in core components
- TLS-by-default for network communications

### Secret Management
- Built-in secret backend support
- Environment variable interpolation
- File-based secret loading
- AWS Secrets Manager integration

## Module System
Vector uses a workspace-based module system with:
- **Core libraries** (`lib/vector-*`): Shared functionality
- **Specialized libraries**: Component-specific logic
- **Feature flags**: Conditional compilation for size optimization

## Best Practices

1. **Component Development**: Follow the three-trait pattern (Config, Builder, Component)
2. **Testing**: Always include unit and integration tests
3. **Documentation**: Update component docs and schemas
4. **Performance**: Profile new components with benchmarks
5. **Security**: Never log or expose sensitive data
6. **Configuration**: Use `#[configurable_component]` for schema generation

## Troubleshooting Development Issues

### Common Build Issues
- **Missing Dependencies**: Run `make environment` for containerized development
- **Cross-compilation Errors**: Ensure Docker/Podman is available
- **Test Failures**: Use `VECTOR_LOG=debug` for detailed logging

### Performance Issues
- **Slow Tests**: Use `make test-unit` for faster unit-only testing
- **Memory Usage**: Enable jemalloc with `unix` feature for better allocation
- **Build Times**: Use `sccache` or build in container for caching

### Integration Test Issues
- **Service Startup**: Increase timeout with `CONTAINER_WAIT_TIME`
- **Docker Issues**: Ensure sufficient resources (4GB+ recommended)
- **Network Problems**: Check firewall/proxy settings for service ports

## Development Patterns

### Adding a New Component
1. Create component module in appropriate directory (`src/sources/`, etc.)
2. Implement required traits (`SourceConfig`, `Source`)
3. Add feature flag in `Cargo.toml`
4. Include in default feature set if appropriate
5. Add integration tests in `tests/integration/`
6. Update documentation

### Configuration Schema
Vector automatically generates JSON schemas for all components:
```rust
#[configurable_component(source("my_source"))]
pub struct MySourceConfig {
    #[configurable(metadata(docs::examples = "example value"))]
    pub setting: String,
}
```

This comprehensive testing and development infrastructure ensures Vector maintains high reliability and performance standards while supporting a vast ecosystem of integrations and deployment scenarios.