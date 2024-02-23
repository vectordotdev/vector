# .PHONY: $(MAKECMDGOALS) all
.DEFAULT_GOAL := help

mkfile_path := $(abspath $(lastword $(MAKEFILE_LIST)))
mkfile_dir := $(dir $(mkfile_path))

# Begin OS detection
ifeq ($(OS),Windows_NT) # is Windows_NT on XP, 2000, 7, Vista, 10...
    export OPERATING_SYSTEM := Windows
    export RUST_TARGET ?= "x86_64-unknown-windows-msvc"
    export FEATURES ?= default-msvc
    undefine DNSTAP_BENCHES
else
    export OPERATING_SYSTEM := $(shell uname)  # same as "uname -s"
    export RUST_TARGET ?= "x86_64-unknown-linux-gnu"
    export FEATURES ?= default
    export DNSTAP_BENCHES := dnstap-benches
endif

# Override this with any scopes for testing/benching.
export SCOPE ?=
# Override this with any extra flags for cargo bench
export CARGO_BENCH_FLAGS ?=
# override this to put criterion output elsewhere
export CRITERION_HOME ?= $(mkfile_dir)target/criterion
# Override to false to disable autospawning services on integration tests.
export AUTOSPAWN ?= true
# Override to control if services are turned off after integration tests.
export AUTODESPAWN ?= ${AUTOSPAWN}
# Override autoinstalling of tools. (Eg `cargo install`)
export AUTOINSTALL ?= false
# Override to true for a bit more log output in your environment building (more coming!)
export VERBOSE ?= false
# Override the container tool. Tries docker first and then tries podman.
export CONTAINER_TOOL ?= auto
ifeq ($(CONTAINER_TOOL),auto)
	ifeq ($(shell docker version >/dev/null 2>&1 && echo docker), docker)
		override CONTAINER_TOOL = docker
	else ifeq ($(shell podman version >/dev/null 2>&1 && echo podman), podman)
		override CONTAINER_TOOL = podman
	else
		override CONTAINER_TOOL = unknown
	endif
endif
# If we're using podman create pods else if we're using docker create networks.
export CURRENT_DIR = $(shell pwd)

# Override this to automatically enter a container containing the correct, full, official build environment for Vector, ready for development
export ENVIRONMENT ?= false
# The upstream container we publish artifacts to on a successful master build.
export ENVIRONMENT_UPSTREAM ?= docker.io/timberio/vector-dev:sha-3eadc96742a33754a5859203b58249f6a806972a
# Override to disable building the container, having it pull from the GitHub packages repo instead
# TODO: Disable this by default. Blocked by `docker pull` from GitHub Packages requiring authenticated login
export ENVIRONMENT_AUTOBUILD ?= true
# Override to disable force pulling the image, leaving the container tool to pull it only when necessary instead
export ENVIRONMENT_AUTOPULL ?= true
# Override this when appropriate to disable a TTY being available in commands with `ENVIRONMENT=true`
export ENVIRONMENT_TTY ?= true
# Override to specify which network the environment will be connected to (leave empty to use the container tool default)
export ENVIRONMENT_NETWORK ?= host
# Override to specify environment port(s) to publish to the host (leave empty to not configure any port publishing)
# Multiple port publishing can be provided using spaces, for example: 8686:8686 8080:8080/udp
export ENVIRONMENT_PUBLISH ?=

# Set dummy AWS credentials if not present - used for AWS and ES integration tests
export AWS_ACCESS_KEY_ID ?= "dummy"
export AWS_SECRET_ACCESS_KEY ?= "dummy"

# Set version
export VERSION ?= $(shell command -v cargo >/dev/null && cargo vdev version || echo unknown)

# Set if you are on the CI and actually want the things to happen. (Non-CI users should never set this.)
export CI ?= false

export RUST_VERSION ?= $(shell grep channel rust-toolchain.toml | cut -d '"' -f 2)

FORMATTING_BEGIN_YELLOW = \033[0;33m
FORMATTING_BEGIN_BLUE = \033[36m
FORMATTING_END = \033[0m

# "One weird trick!" https://www.gnu.org/software/make/manual/make.html#Syntax-of-Functions
EMPTY:=
SPACE:= ${EMPTY} ${EMPTY}
COMMA:= ,

help:
	@printf -- "${FORMATTING_BEGIN_BLUE}                                      __   __  __${FORMATTING_END}\n"
	@printf -- "${FORMATTING_BEGIN_BLUE}                                      \ \ / / / /${FORMATTING_END}\n"
	@printf -- "${FORMATTING_BEGIN_BLUE}                                       \ V / / / ${FORMATTING_END}\n"
	@printf -- "${FORMATTING_BEGIN_BLUE}                                        \_/  \/  ${FORMATTING_END}\n"
	@printf -- "\n"
	@printf -- "                                      V E C T O R\n"
	@printf -- "\n"
	@printf -- "---------------------------------------------------------------------------------------\n"
	@printf -- "Want to use ${FORMATTING_BEGIN_YELLOW}\`docker\`${FORMATTING_END} or ${FORMATTING_BEGIN_YELLOW}\`podman\`${FORMATTING_END}? See ${FORMATTING_BEGIN_YELLOW}\`ENVIRONMENT=true\`${FORMATTING_END} commands. (Default ${FORMATTING_BEGIN_YELLOW}\`CONTAINER_TOOL=docker\`${FORMATTING_END})\n"
	@printf -- "\n"
	@awk 'BEGIN {FS = ":.*##"; printf "Usage: make ${FORMATTING_BEGIN_BLUE}<target>${FORMATTING_END}\n"} /^[a-zA-Z0-9_-]+:.*?##/ { printf "  ${FORMATTING_BEGIN_BLUE}%-46s${FORMATTING_END} %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Environment

# These are some predefined macros, please use them!
ifeq ($(ENVIRONMENT), true)
define MAYBE_ENVIRONMENT_EXEC
${ENVIRONMENT_EXEC}
endef
else
define MAYBE_ENVIRONMENT_EXEC

endef
endif

ifeq ($(ENVIRONMENT), true)
define MAYBE_ENVIRONMENT_COPY_ARTIFACTS
${ENVIRONMENT_COPY_ARTIFACTS}
endef
else
define MAYBE_ENVIRONMENT_COPY_ARTIFACTS

endef
endif

# We use a volume here as non-Linux hosts are extremely slow to share disks, and Linux hosts tend to get permissions clobbered.
define ENVIRONMENT_EXEC
	${ENVIRONMENT_PREPARE}
	@echo "Entering environment..."
	@mkdir -p target
	$(CONTAINER_TOOL) run \
			--name vector-environment \
			--rm \
			$(if $(findstring true,$(ENVIRONMENT_TTY)),--tty,) \
			--init \
			--interactive \
			--env INSIDE_ENVIRONMENT=true \
			$(if $(ENVIRONMENT_NETWORK),--network $(ENVIRONMENT_NETWORK),) \
			--mount type=bind,source=${CURRENT_DIR},target=/git/vectordotdev/vector \
			$(if $(findstring docker,$(CONTAINER_TOOL)),--mount type=bind$(COMMA)source=/var/run/docker.sock$(COMMA)target=/var/run/docker.sock,) \
			--mount type=volume,source=vector-target,target=/git/vectordotdev/vector/target \
			--mount type=volume,source=vector-cargo-cache,target=/root/.cargo \
			--mount type=volume,source=vector-rustup-cache,target=/root/.rustup \
			$(foreach publish,$(ENVIRONMENT_PUBLISH),--publish $(publish)) \
			$(ENVIRONMENT_UPSTREAM)
endef


ifneq ($(CONTAINER_TOOL), unknown)
ifeq ($(ENVIRONMENT_AUTOBUILD), true)
define ENVIRONMENT_PREPARE
	@echo "Building the environment. (ENVIRONMENT_AUTOBUILD=true) This may take a few minutes..."
	$(CONTAINER_TOOL) build \
		$(if $(findstring true,$(VERBOSE)),,--quiet) \
		--tag $(ENVIRONMENT_UPSTREAM) \
		--file scripts/environment/Dockerfile \
		.
endef
else ifeq ($(ENVIRONMENT_AUTOPULL), true)
define ENVIRONMENT_PREPARE
	@echo "Pulling the environment image. (ENVIRONMENT_AUTOPULL=true)"
	$(CONTAINER_TOOL) pull $(ENVIRONMENT_UPSTREAM)
endef
endif
else
define ENVIRONMENT_PREPARE
$(error "Please install a container tool such as Docker or Podman")
endef
endif

.PHONY: check-container-tool
check-container-tool: ## Checks what container tool is installed
	@echo -n "Checking if $(CONTAINER_TOOL) is available..." && \
	$(CONTAINER_TOOL) version 1>/dev/null && echo "yes"

.PHONY: environment
environment: export ENVIRONMENT_TTY = true ## Enter a full Vector dev shell in $CONTAINER_TOOL, binding this folder to the container.
environment:
	${ENVIRONMENT_EXEC}

.PHONY: environment-prepare
environment-prepare: ## Prepare the Vector dev shell using $CONTAINER_TOOL.
	${ENVIRONMENT_PREPARE}

.PHONY: environment-clean
environment-clean: ## Clean the Vector dev shell using $CONTAINER_TOOL.
	@$(CONTAINER_TOOL) volume rm -f vector-target vector-cargo-cache vector-rustup-cache
	@$(CONTAINER_TOOL) rmi $(ENVIRONMENT_UPSTREAM) || true

.PHONY: environment-push
environment-push: environment-prepare ## Publish a new version of the container image.
	$(CONTAINER_TOOL) push $(ENVIRONMENT_UPSTREAM)

##@ Building
.PHONY: build
build: check-build-tools
build: export CFLAGS += -g0 -O3
build: ## Build the project in release mode (Supports `ENVIRONMENT=true`)
	${MAYBE_ENVIRONMENT_EXEC} cargo build --release --no-default-features --features ${FEATURES}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: build-dev
build-dev: ## Build the project in development mode (Supports `ENVIRONMENT=true`)
	${MAYBE_ENVIRONMENT_EXEC} cargo build --no-default-features --features ${FEATURES}

.PHONY: build-x86_64-unknown-linux-gnu
build-x86_64-unknown-linux-gnu: target/x86_64-unknown-linux-gnu/release/vector ## Build a release binary for the x86_64-unknown-linux-gnu triple.
	@echo "Output to ${<}"

.PHONY: build-aarch64-unknown-linux-gnu
build-aarch64-unknown-linux-gnu: target/aarch64-unknown-linux-gnu/release/vector ## Build a release binary for the aarch64-unknown-linux-gnu triple.
	@echo "Output to ${<}"

.PHONY: build-x86_64-unknown-linux-musl
build-x86_64-unknown-linux-musl: target/x86_64-unknown-linux-musl/release/vector ## Build a release binary for the x86_64-unknown-linux-musl triple.
	@echo "Output to ${<}"

.PHONY: build-aarch64-unknown-linux-musl
build-aarch64-unknown-linux-musl: target/aarch64-unknown-linux-musl/release/vector ## Build a release binary for the aarch64-unknown-linux-musl triple.
	@echo "Output to ${<}"

.PHONY: build-armv7-unknown-linux-gnueabihf
build-armv7-unknown-linux-gnueabihf: target/armv7-unknown-linux-gnueabihf/release/vector ## Build a release binary for the armv7-unknown-linux-gnueabihf triple.
	@echo "Output to ${<}"

.PHONY: build-armv7-unknown-linux-musleabihf
build-armv7-unknown-linux-musleabihf: target/armv7-unknown-linux-musleabihf/release/vector ## Build a release binary for the armv7-unknown-linux-musleabihf triple.
	@echo "Output to ${<}"

.PHONY: build-graphql-schema
build-graphql-schema: ## Generate the `schema.json` for Vector's GraphQL API
	${MAYBE_ENVIRONMENT_EXEC} cargo run --bin graphql-schema --no-default-features --features=default-no-api-client

.PHONY: check-build-tools
check-build-tools:
ifneq ($(ENVIRONMENT), true)
ifeq ($(shell command -v cargo >/dev/null || echo not-found), not-found)
	$(error "Please install Rust: https://www.rust-lang.org/tools/install")
endif
endif

##@ Cross Compiling
.PHONY: cross-enable
cross-enable: cargo-install-cross

.PHONY: CARGO_HANDLES_FRESHNESS
CARGO_HANDLES_FRESHNESS:
	${EMPTY}

# GNU Make < 3.82 pattern matching priority depends on the definition order
# so cross-image-% must be defined before cross-%
.PHONY: cross-image-%
cross-image-%: export TRIPLE =$($(strip @):cross-image-%=%)
cross-image-%:
	$(CONTAINER_TOOL) build \
		--tag vector-cross-env:${TRIPLE} \
		--file scripts/cross/${TRIPLE}.dockerfile \
		.

# This is basically a shorthand for folks.
# `cross-anything-triple` will call `cross anything --target triple` with the right features.
.PHONY: cross-%
cross-%: export PAIR =$(subst -, ,$($(strip @):cross-%=%))
cross-%: export COMMAND ?=$(word 1,${PAIR})
cross-%: export TRIPLE ?=$(subst ${SPACE},-,$(wordlist 2,99,${PAIR}))
cross-%: export PROFILE ?= release
cross-%: export CFLAGS += -g0 -O3
cross-%: cargo-install-cross
	$(MAKE) -k cross-image-${TRIPLE}
	cross ${COMMAND} \
		$(if $(findstring release,$(PROFILE)),--release,) \
		--target ${TRIPLE} \
		--no-default-features \
		--features target-${TRIPLE}

target/%/vector: export PAIR =$(subst /, ,$(@:target/%/vector=%))
target/%/vector: export TRIPLE ?=$(word 1,${PAIR})
target/%/vector: export PROFILE ?=$(word 2,${PAIR})
target/%/vector: export CFLAGS += -g0 -O3
target/%/vector: cargo-install-cross CARGO_HANDLES_FRESHNESS
	$(MAKE) -k cross-image-${TRIPLE}
	cross build \
		$(if $(findstring release,$(PROFILE)),--release,) \
		--target ${TRIPLE} \
		--no-default-features \
		--features target-${TRIPLE}

target/%/vector.tar.gz: export PAIR =$(subst /, ,$(@:target/%/vector.tar.gz=%))
target/%/vector.tar.gz: export TRIPLE ?=$(word 1,${PAIR})
target/%/vector.tar.gz: export PROFILE ?=$(word 2,${PAIR})
target/%/vector.tar.gz: target/%/vector CARGO_HANDLES_FRESHNESS
	rm -rf target/scratch/vector-${TRIPLE} || true
	mkdir -p target/scratch/vector-${TRIPLE}/bin target/scratch/vector-${TRIPLE}/etc
	cp -R -f -v \
		target/${TRIPLE}/${PROFILE}/vector \
		target/scratch/vector-${TRIPLE}/bin/vector
	cp -R -f -v \
		README.md \
		LICENSE \
		licenses \
		NOTICE \
		LICENSE-3rdparty.csv \
		config \
		target/scratch/vector-${TRIPLE}/
	cp -R -f -v \
		distribution/systemd \
		target/scratch/vector-${TRIPLE}/etc/
	tar --create \
		--gzip \
		--verbose \
		--file target/${TRIPLE}/${PROFILE}/vector.tar.gz \
		--directory target/scratch/ \
		./vector-${TRIPLE}
	rm -rf target/scratch/

##@ Testing (Supports `ENVIRONMENT=true`)

# nextest doesn't support running doc tests yet so this is split out as
# `test-docs`
# https://github.com/nextest-rs/nextest/issues/16
#
# criterion doesn't support the flags needed by nextest to run so these are left
# out for now
# https://github.com/bheisler/criterion.rs/issues/562
#
# `cargo test` lacks support for testing _just_ benches otherwise we'd have
# a target for that
# https://github.com/rust-lang/cargo/issues/6454
.PHONY: test
test: ## Run the unit test suite
	${MAYBE_ENVIRONMENT_EXEC} cargo nextest run --workspace --no-fail-fast --no-default-features --features "${FEATURES}" ${SCOPE}

.PHONY: test-docs
test-docs: ## Run the docs test suite
	${MAYBE_ENVIRONMENT_EXEC} cargo test --doc --workspace --no-fail-fast --no-default-features --features "${FEATURES}" ${SCOPE}

.PHONY: test-all
test-all: test test-docs test-behavior test-integration test-component-validation ## Runs all tests: unit, docs, behavioral, integration, and component validation.

.PHONY: test-x86_64-unknown-linux-gnu
test-x86_64-unknown-linux-gnu: cross-test-x86_64-unknown-linux-gnu ## Runs unit tests on the x86_64-unknown-linux-gnu triple
	${EMPTY}

.PHONY: test-aarch64-unknown-linux-gnu
test-aarch64-unknown-linux-gnu: cross-test-aarch64-unknown-linux-gnu ## Runs unit tests on the aarch64-unknown-linux-gnu triple
	${EMPTY}

.PHONY: test-behavior-config
test-behavior-config: ## Runs configuration related behavioral tests
	${MAYBE_ENVIRONMENT_EXEC} cargo build --no-default-features --features secret-backend-example --bin secret-backend-example
	${MAYBE_ENVIRONMENT_EXEC} cargo run --no-default-features --features transforms -- test tests/behavior/config/*

.PHONY: test-behavior-%
test-behavior-%: ## Runs behavioral test for a given category
	${MAYBE_ENVIRONMENT_EXEC} cargo run --no-default-features --features transforms -- test tests/behavior/$*/*

.PHONY: test-behavior
test-behavior: ## Runs all behavioral tests
test-behavior: test-behavior-transforms test-behavior-formats test-behavior-config

.PHONY: test-integration
test-integration: ## Runs all integration tests
test-integration: test-integration-amqp test-integration-appsignal test-integration-aws test-integration-axiom test-integration-azure test-integration-chronicle test-integration-clickhouse
test-integration: test-integration-databend test-integration-docker-logs test-integration-elasticsearch
test-integration: test-integration-eventstoredb test-integration-fluent test-integration-gcp test-integration-greptimedb test-integration-humio test-integration-http-client test-integration-influxdb
test-integration: test-integration-kafka test-integration-logstash test-integration-loki test-integration-mongodb test-integration-nats
test-integration: test-integration-nginx test-integration-opentelemetry test-integration-postgres test-integration-prometheus test-integration-pulsar
test-integration: test-integration-redis test-integration-splunk test-integration-dnstap test-integration-datadog-agent test-integration-datadog-logs test-integration-e2e-datadog-logs
test-integration: test-integration-datadog-traces test-integration-shutdown

test-integration-%-cleanup:
	cargo vdev --verbose integration stop $*

test-integration-%:
	cargo vdev --verbose integration test $*
ifeq ($(AUTODESPAWN), true)
	make test-integration-$*-cleanup
endif

.PHONY: test-e2e-kubernetes
test-e2e-kubernetes: ## Runs Kubernetes E2E tests (Sorry, no `ENVIRONMENT=true` support)
	RUST_VERSION=${RUST_VERSION} scripts/test-e2e-kubernetes.sh

.PHONY: test-cli
test-cli: ## Runs cli tests
	${MAYBE_ENVIRONMENT_EXEC} cargo nextest run --no-fail-fast --no-default-features --features cli-tests --test integration --test-threads 4

.PHONY: test-component-validation
test-component-validation: ## Runs component validation tests
	${MAYBE_ENVIRONMENT_EXEC} cargo nextest run --no-fail-fast --no-default-features --features component-validation-tests --status-level pass --test-threads 4 components::validation::tests

##@ Benching (Supports `ENVIRONMENT=true`)

.PHONY: bench
bench: ## Run benchmarks in /benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "benches" ${CARGO_BENCH_FLAGS}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: bench-dnstap
bench-dnstap: ## Run dnstap benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "dnstap-benches" --bench dnstap ${CARGO_BENCH_FLAGS}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: bench-dnsmsg-parser
bench-dnsmsg-parser: ## Run dnsmsg-parser benches
	${MAYBE_ENVIRONMENT_EXEC} CRITERION_HOME="$(CRITERION_HOME)" cargo bench --manifest-path lib/dnsmsg-parser/Cargo.toml ${CARGO_BENCH_FLAGS}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: bench-remap-functions
bench-remap-functions: ## Run remap-functions benches
	${MAYBE_ENVIRONMENT_EXEC} CRITERION_HOME="$(CRITERION_HOME)" cargo bench --manifest-path lib/vrl/stdlib/Cargo.toml ${CARGO_BENCH_FLAGS}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: bench-remap
bench-remap: ## Run remap benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "remap-benches" --bench remap ${CARGO_BENCH_FLAGS}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: bench-transform
bench-transform: ## Run transform benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "transform-benches" --bench transform ${CARGO_BENCH_FLAGS}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: bench-languages
bench-languages:  ### Run language comparison benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "language-benches" --bench languages ${CARGO_BENCH_FLAGS}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: bench-metrics
bench-metrics: ## Run metrics benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "metrics-benches" ${CARGO_BENCH_FLAGS}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: bench-all
bench-all: ### Run all benches
bench-all: bench-remap-functions
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "benches remap-benches  metrics-benches language-benches ${DNSTAP_BENCHES}" ${CARGO_BENCH_FLAGS}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

##@ Checking

.PHONY: check
check: ## Run prerequisite code checks
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check rust

.PHONY: check-all
check-all: ## Check everything
check-all: check-fmt check-clippy check-docs
check-all: check-version check-examples check-component-features
check-all: check-scripts check-deny check-component-docs check-licenses

.PHONY: check-component-features
check-component-features: ## Check that all component features are setup properly
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check component-features

.PHONY: check-clippy
check-clippy: ## Check code with Clippy
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check rust --clippy

.PHONY: check-docs
check-docs: ## Check that all /docs file are valid
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check docs

.PHONY: check-fmt
check-fmt: ## Check that all files are formatted properly
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check fmt

.PHONY: check-licenses
check-licenses: ## Check that the 3rd-party license file is up to date
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check licenses

.PHONY: check-markdown
check-markdown: ## Check that markdown is styled properly
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check markdown

.PHONY: check-version
check-version: ## Check that Vector's version is correct accounting for recent changes
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check version

.PHONY: check-examples
check-examples: ## Check that the config/examples files are valid
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check examples

.PHONY: check-scripts
check-scripts: ## Check that scripts do not have common mistakes
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check scripts

.PHONY: check-deny
check-deny: ## Check advisories licenses and sources for crate dependencies
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check deny

.PHONY: check-events
check-events: ## Check that events satisfy patterns set in https://github.com/vectordotdev/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check events

.PHONY: check-component-docs
check-component-docs: generate-component-docs ## Checks that the machine-generated component Cue docs are up-to-date.
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev check component-docs

##@ Rustdoc
build-rustdoc: ## Build Vector's Rustdocs
	# This command is mostly intended for use by the build process in vectordotdev/vector-rustdoc
	${MAYBE_ENVIRONMENT_EXEC} cargo doc --no-deps --workspace

##@ Packaging

# archives
target/artifacts/vector-${VERSION}-%.tar.gz: export TRIPLE :=$(@:target/artifacts/vector-${VERSION}-%.tar.gz=%)
target/artifacts/vector-${VERSION}-%.tar.gz: override PROFILE =release
target/artifacts/vector-${VERSION}-%.tar.gz: target/%/release/vector.tar.gz
	@echo "Built to ${<}, relocating to ${@}"
	@mkdir -p target/artifacts/
	@cp -v \
		${<} \
		${@}

.PHONY: package
package: build ## Build the Vector archive
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev package archive

.PHONY: package-x86_64-unknown-linux-gnu-all
package-x86_64-unknown-linux-gnu-all: package-x86_64-unknown-linux-gnu package-deb-x86_64-unknown-linux-gnu package-rpm-x86_64-unknown-linux-gnu # Build all x86_64 GNU packages

.PHONY: package-x86_64-unknown-linux-musl-all
package-x86_64-unknown-linux-musl-all: package-x86_64-unknown-linux-musl # Build all x86_64 MUSL packages

.PHONY: package-aarch64-unknown-linux-musl-all
package-aarch64-unknown-linux-musl-all: package-aarch64-unknown-linux-musl # Build all aarch64 MUSL packages

.PHONY: package-aarch64-unknown-linux-gnu-all
package-aarch64-unknown-linux-gnu-all: package-aarch64-unknown-linux-gnu package-deb-aarch64 package-rpm-aarch64 # Build all aarch64 GNU packages

.PHONY: package-armv7-unknown-linux-gnueabihf-all
package-armv7-unknown-linux-gnueabihf-all: package-armv7-unknown-linux-gnueabihf package-deb-armv7-gnu package-rpm-armv7hl-gnu  # Build all armv7-unknown-linux-gnueabihf MUSL packages

.PHONY: package-x86_64-unknown-linux-gnu
package-x86_64-unknown-linux-gnu: target/artifacts/vector-${VERSION}-x86_64-unknown-linux-gnu.tar.gz ## Build an archive suitable for the `x86_64-unknown-linux-gnu` triple.
	@echo "Output to ${<}."

.PHONY: package-x86_64-unknown-linux-musl
package-x86_64-unknown-linux-musl: target/artifacts/vector-${VERSION}-x86_64-unknown-linux-musl.tar.gz ## Build an archive suitable for the `x86_64-unknown-linux-musl` triple.
	@echo "Output to ${<}."

.PHONY: package-aarch64-unknown-linux-musl
package-aarch64-unknown-linux-musl: target/artifacts/vector-${VERSION}-aarch64-unknown-linux-musl.tar.gz ## Build an archive suitable for the `aarch64-unknown-linux-musl` triple.
	@echo "Output to ${<}."

.PHONY: package-aarch64-unknown-linux-gnu
package-aarch64-unknown-linux-gnu: target/artifacts/vector-${VERSION}-aarch64-unknown-linux-gnu.tar.gz ## Build an archive suitable for the `aarch64-unknown-linux-gnu` triple.
	@echo "Output to ${<}."

.PHONY: package-armv7-unknown-linux-gnueabihf
package-armv7-unknown-linux-gnueabihf: target/artifacts/vector-${VERSION}-armv7-unknown-linux-gnueabihf.tar.gz ## Build an archive suitable for the `armv7-unknown-linux-gnueabihf` triple.
	@echo "Output to ${<}."

.PHONY: package-armv7-unknown-linux-musleabihf
package-armv7-unknown-linux-musleabihf: target/artifacts/vector-${VERSION}-armv7-unknown-linux-musleabihf.tar.gz ## Build an archive suitable for the `armv7-unknown-linux-musleabihf triple.
	@echo "Output to ${<}."

# debs

.PHONY: package-deb-x86_64-unknown-linux-gnu
package-deb-x86_64-unknown-linux-gnu: package-x86_64-unknown-linux-gnu ## Build the x86_64 GNU deb package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/vectordotdev/vector/ -e TARGET=x86_64-unknown-linux-gnu -e VECTOR_VERSION $(ENVIRONMENT_UPSTREAM) cargo vdev package deb

.PHONY: package-deb-x86_64-unknown-linux-musl
package-deb-x86_64-unknown-linux-musl: package-x86_64-unknown-linux-musl ## Build the x86_64 GNU deb package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/vectordotdev/vector/ -e TARGET=x86_64-unknown-linux-musl -e VECTOR_VERSION $(ENVIRONMENT_UPSTREAM) cargo vdev package deb

.PHONY: package-deb-aarch64
package-deb-aarch64: package-aarch64-unknown-linux-gnu ## Build the aarch64 deb package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/vectordotdev/vector/ -e TARGET=aarch64-unknown-linux-gnu -e VECTOR_VERSION $(ENVIRONMENT_UPSTREAM) cargo vdev package deb

.PHONY: package-deb-armv7-gnu
package-deb-armv7-gnu: package-armv7-unknown-linux-gnueabihf ## Build the armv7-unknown-linux-gnueabihf deb package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/vectordotdev/vector/ -e TARGET=armv7-unknown-linux-gnueabihf -e VECTOR_VERSION $(ENVIRONMENT_UPSTREAM) cargo vdev package deb

# rpms

.PHONY: package-rpm-x86_64-unknown-linux-gnu
package-rpm-x86_64-unknown-linux-gnu: package-x86_64-unknown-linux-gnu ## Build the x86_64 rpm package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/vectordotdev/vector/ -e TARGET=x86_64-unknown-linux-gnu -e VECTOR_VERSION $(ENVIRONMENT_UPSTREAM) cargo vdev package rpm

.PHONY: package-rpm-x86_64-unknown-linux-musl
package-rpm-x86_64-unknown-linux-musl: package-x86_64-unknown-linux-musl ## Build the x86_64 musl rpm package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/vectordotdev/vector/ -e TARGET=x86_64-unknown-linux-musl -e VECTOR_VERSION $(ENVIRONMENT_UPSTREAM) cargo vdev package rpm

.PHONY: package-rpm-aarch64
package-rpm-aarch64: package-aarch64-unknown-linux-gnu ## Build the aarch64 rpm package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/vectordotdev/vector/ -e TARGET=aarch64-unknown-linux-gnu -e VECTOR_VERSION $(ENVIRONMENT_UPSTREAM) cargo vdev package rpm

.PHONY: package-rpm-armv7hl-gnu
package-rpm-armv7hl-gnu: package-armv7-unknown-linux-gnueabihf ## Build the armv7hl-unknown-linux-gnueabihf rpm package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/vectordotdev/vector/ -e TARGET=armv7-unknown-linux-gnueabihf -e ARCH=armv7hl -e VECTOR_VERSION $(ENVIRONMENT_UPSTREAM) cargo vdev package rpm

##@ Releasing

.PHONY: release
release: release-prepare generate release-commit ## Release a new Vector version

.PHONY: release-commit
release-commit: ## Commits release changes
	@cargo vdev release commit

.PHONY: release-docker
release-docker: ## Release to Docker Hub
	@cargo vdev release docker

.PHONY: release-github
release-github: ## Release to GitHub
	@cargo vdev release github

.PHONY: release-homebrew
release-homebrew: ## Release to vectordotdev Homebrew tap
	@cargo vdev release homebrew

.PHONY: release-prepare
release-prepare: ## Prepares the release with metadata and highlights
	@cargo vdev release prepare

.PHONY: release-push
release-push: ## Push new Vector version
	@cargo vdev release push

.PHONY: release-s3
release-s3: ## Release artifacts to S3
	@cargo vdev release s3

.PHONY: sync-install
sync-install: ## Sync the install.sh script for access via sh.vector.dev
	@aws s3 cp distribution/install.sh s3://sh.vector.dev --sse --acl public-read

.PHONY: sha256sum
sha256sum: ## Generate SHA256 checksums of CI artifacts
	scripts/checksum.sh

##@ Vector Remap Language

.PHONY: test-vrl
test-vrl: ## Run the VRL test suite
	@cargo vdev test-vrl

.PHONY: compile-vrl-wasm
compile-vrl-wasm: ## Compile VRL crates to WASM target
	cargo vdev build vrl-wasm

##@ Utility

.PHONY: clean
clean: environment-clean ## Clean everything
	cargo clean

.PHONY: fmt
fmt: ## Format code
	${MAYBE_ENVIRONMENT_EXEC} cargo fmt

.PHONY: generate-kubernetes-manifests
generate-kubernetes-manifests: ## Generate Kubernetes manifests from latest Helm chart
	cargo vdev build manifests

.PHONY: generate-component-docs
generate-component-docs: ## Generate per-component Cue docs from the configuration schema.
	${MAYBE_ENVIRONMENT_EXEC} cargo build $(if $(findstring true,$(CI)),--quiet,)
	target/debug/vector generate-schema > /tmp/vector-config-schema.json 2>/dev/null
	${MAYBE_ENVIRONMENT_EXEC} cargo vdev build component-docs /tmp/vector-config-schema.json \
		$(if $(findstring true,$(CI)),>/dev/null,)

.PHONY: signoff
signoff: ## Signsoff all previous commits since branch creation
	scripts/signoff.sh

.PHONY: version
version: ## Get the current Vector version
	@cargo vdev version

.PHONY: git-hooks
git-hooks: ## Add Vector-local git hooks for commit sign-off
	@scripts/install-git-hooks.sh

.PHONY: cargo-install-%
cargo-install-%: override TOOL = $(@:cargo-install-%=%)
cargo-install-%:
	$(if $(findstring true,$(AUTOINSTALL)),cargo install ${TOOL} --quiet; cargo clean,)

.PHONY: ci-generate-publish-metadata
ci-generate-publish-metadata: ## Generates the necessary metadata required for building/publishing Vector.
	cargo vdev build publish-metadata
