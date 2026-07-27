# .PHONY: $(MAKECMDGOALS) all
.DEFAULT_GOAL := help

mkfile_path := $(abspath $(lastword $(MAKEFILE_LIST)))
mkfile_dir := $(dir $(mkfile_path))

# Make project-local npm tools (installed by scripts/environment/prepare.sh on
# laptops) discoverable to recipes without requiring contributors to edit PATH.
# CI installs the same tools globally and is unaffected by the prefix.
export PATH := $(mkfile_dir)scripts/environment/npm-tools/node_modules/.bin:$(PATH)

# Begin OS detection
ifeq ($(OS),Windows_NT) # is Windows_NT on XP, 2000, 7, Vista, 10...
    export OPERATING_SYSTEM := Windows
    export RUST_TARGET ?= "x86_64-unknown-windows-msvc"
    undefine DNSTAP_BENCHES
else
    export OPERATING_SYSTEM := $(shell uname)  # same as "uname -s"
    export RUST_TARGET ?= "x86_64-unknown-linux-gnu"
    export DNSTAP_BENCHES := dnstap-benches
endif
export FEATURES ?=

# When COVERAGE=true, swap cargo-nextest for cargo-llvm-cov so test targets collect
# coverage data. Run `make coverage-report` afterwards to emit the lcov file.
export COVERAGE ?= false
ifeq ($(COVERAGE), true)
TEST_RUNNER := cargo llvm-cov nextest --no-report
else
TEST_RUNNER := cargo nextest run
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
CONTAINER_TOOL ?= $(shell docker version >/dev/null 2>&1 && echo docker || (podman version >/dev/null 2>&1 && echo podman) || echo unknown)
# If we're using podman create pods else if we're using docker create networks.
export CURRENT_DIR = $(shell pwd)

# Preserve any caller-supplied VDEV (e.g. CI exports the pinned prebuilt binary
# via .github/actions/setup; falling back to `cargo vdev` recompiles vdev).
VDEV ?= cargo vdev

# Set dummy AWS credentials if not present - used for AWS and ES integration tests
export AWS_ACCESS_KEY_ID ?= "dummy"
export AWS_SECRET_ACCESS_KEY ?= "dummy"

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
	@printf -- "Default ${FORMATTING_BEGIN_YELLOW}\`CONTAINER_TOOL=docker\`${FORMATTING_END} (auto-detects ${FORMATTING_BEGIN_YELLOW}\`docker\`${FORMATTING_END} or ${FORMATTING_BEGIN_YELLOW}\`podman\`${FORMATTING_END}).\n"
	@printf -- "\n"
	@awk 'BEGIN {FS = ":.*##"; printf "Usage: make ${FORMATTING_BEGIN_BLUE}<target>${FORMATTING_END}\n"} /^[a-zA-Z0-9_-]+:.*?##/ { printf "  ${FORMATTING_BEGIN_BLUE}%-46s${FORMATTING_END} %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Building
.PHONY: build
build: check-build-tools
build: export CFLAGS += -g0 -O3
build: ## Build the project in release mode
	cargo build --release $(if $(FEATURES),--no-default-features --features "$(FEATURES)")

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

.PHONY: build-arm-unknown-linux-gnueabi
build-arm-unknown-linux-gnueabi: target/arm-unknown-linux-gnueabi/release/vector ## Build a release binary for the arm-unknown-linux-gnueabi triple.
	@echo "Output to ${<}"

.PHONY: build-arm-unknown-linux-musleabi
build-arm-unknown-linux-musleabi: target/arm-unknown-linux-musleabi/release/vector ## Build a release binary for the arm-unknown-linux-musleabi triple.
	@echo "Output to ${<}"

.PHONY: check-build-tools
check-build-tools:
ifeq ($(shell command -v cargo >/dev/null || echo not-found), not-found)
	$(error "Please install Rust: https://www.rust-lang.org/tools/install")
endif

##@ Cross Compiling
.PHONY: cross-enable
cross-enable: cargo-install-cross

.PHONY: CARGO_HANDLES_FRESHNESS
CARGO_HANDLES_FRESHNESS:
	${EMPTY}

# Pinned digests for ghcr.io/cross-rs/<target>:edge.
# Refresh with: crane digest ghcr.io/cross-rs/<target>:edge
CROSS_DIGEST_x86_64-unknown-linux-gnu       := sha256:13f7a68e55cb05a19e840bce65834fc785dc069e0c2218d12b8fdb8f8a1519d5
CROSS_DIGEST_aarch64-unknown-linux-gnu      := sha256:3bf094d22fc4f73c9bdce45ddd7a8bbae349efdbd51b4d4b5ee1bedd8454466b
CROSS_DIGEST_x86_64-unknown-linux-musl      := sha256:c59deede3efcd7cb6f6a57641241ba1c63cfe35b7965be09a851242b4209639d
CROSS_DIGEST_aarch64-unknown-linux-musl     := sha256:dad492e0f040c6e712d4be9b970c9de5f3b8ef9cde6b9a2b437d56d1dabeb808
CROSS_DIGEST_armv7-unknown-linux-gnueabihf  := sha256:73294ebb06e077e49bbbecfe8f17507e9e0b733a2a1ba23056abcd9c0ba617c9
CROSS_DIGEST_armv7-unknown-linux-musleabihf := sha256:49bdc9a4cf2f1bcb385389c85be8f43c4399fa6d6fe22883702ef13eb921e443
CROSS_DIGEST_arm-unknown-linux-gnueabi      := sha256:0c70b0e54724bd599dff00a2888f8ea176a5b6c85af47aad9ad25296f63e2967
CROSS_DIGEST_arm-unknown-linux-musleabi     := sha256:0ca8f4afcc29fb5964aa63e482452e8869311a610c5868f22ded400c4e483328

# GNU Make < 3.82 pattern matching priority depends on the definition order
# so cross-image-% must be defined before cross-%
.PHONY: cross-image-%
cross-image-%: export TRIPLE =$($(strip @):cross-image-%=%)
cross-image-%:
	@if [ -n "$(CROSS_DIGEST_$*)" ]; then \
		$(CONTAINER_TOOL) build \
			--build-arg TARGET=$* \
			--build-arg CROSS_DIGEST=$(CROSS_DIGEST_$*) \
			--file scripts/cross/Dockerfile \
			--tag vector-cross-env:$* \
			. ; \
	else \
		echo "No image digest pinned for $*. Add it to the digest table in Makefile." >&2 ; \
		exit 1 ; \
	fi

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
ifeq ($(NATIVE),true)
target/%/vector: CARGO_HANDLES_FRESHNESS
	cargo build \
		$(if $(findstring release,$(PROFILE)),--release,) \
		--target ${TRIPLE} \
		--no-default-features \
		--features target-${TRIPLE}
else
target/%/vector: cargo-install-cross CARGO_HANDLES_FRESHNESS
	$(MAKE) -k cross-image-${TRIPLE}
	cross build \
		$(if $(findstring release,$(PROFILE)),--release,) \
		--target ${TRIPLE} \
		--no-default-features \
		--features target-${TRIPLE}
endif

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

##@ Testing

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
	${TEST_RUNNER} --workspace --no-fail-fast $(if $(FEATURES),--no-default-features --features "$(FEATURES)") ${SCOPE}

.PHONY: test-docs
test-docs: ## Run the docs test suite
	cargo test --doc --workspace --no-fail-fast $(if $(FEATURES),--no-default-features --features "$(FEATURES)") ${SCOPE}

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
	cargo build --no-default-features --features secret-backend-example --bin secret-backend-example
	cargo run --no-default-features --features transforms -- test tests/behavior/config/*

.PHONY: test-behavior-%
test-behavior-%: ## Runs behavioral test for a given category
	cargo run --no-default-features --features transforms,vrl-functions-env,vrl-functions-system,vrl-functions-network,vrl-functions-crypto -- test tests/behavior/$*/*

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
test-integration: test-integration-redis test-integration-splunk test-integration-dnstap test-integration-datadog-agent test-integration-datadog-logs test-integration-e2e-datadog-logs test-integration-e2e-opentelemetry-logs
test-integration: test-integration-datadog-traces test-integration-shutdown

.PHONY: test-integration-windows-event-log
test-integration-windows-event-log: ## Runs Windows Event Log integration tests (Windows only)
ifeq ($(OS),Windows_NT)
	cargo test -p vector --no-default-features --features sources-windows_event_log-integration-tests windows_event_log::integration_tests
else
	@echo "Skipping windows-event-log integration tests (Windows only)"
endif

test-integration-%-cleanup:
	$(VDEV) --verbose integration stop $*

test-integration-%:
	$(VDEV) --verbose integration test $*
ifeq ($(AUTODESPAWN), true)
	make test-integration-$*-cleanup
endif

.PHONY: test-e2e-kubernetes
test-e2e-kubernetes: ## Runs Kubernetes E2E tests
	RUST_VERSION=${RUST_VERSION} scripts/test-e2e-kubernetes.sh

.PHONY: test-cli
test-cli: ## Runs cli tests
	${TEST_RUNNER} --no-fail-fast --no-default-features --features cli-tests --test integration --test-threads 4

.PHONY: test-vector-api
test-vector-api: ## Runs vector API tests (top and tap)
	${TEST_RUNNER} --no-fail-fast --no-default-features --features vector-api-tests --test vector_api

.PHONY: test-component-validation
test-component-validation: ## Runs component validation tests
	${TEST_RUNNER} --no-fail-fast --no-default-features --features component-validation-tests --status-level pass --test-threads 4 --lib components::validation::tests

.PHONY: coverage-report
coverage-report: ## Generate lcov report after running tests with COVERAGE=true (outputs lcov.info)
	cargo llvm-cov report --lcov --output-path lcov.info

##@ Benching

.PHONY: bench
bench: ## Run benchmarks in /benches
	cargo bench --no-default-features --features "benches" ${CARGO_BENCH_FLAGS}

.PHONY: bench-dnstap
bench-dnstap: ## Run dnstap benches
	cargo bench --no-default-features --features "dnstap-benches" --bench dnstap ${CARGO_BENCH_FLAGS}

.PHONY: bench-dnsmsg-parser
bench-dnsmsg-parser: ## Run dnsmsg-parser benches
	CRITERION_HOME="$(CRITERION_HOME)" cargo bench --manifest-path lib/dnsmsg-parser/Cargo.toml ${CARGO_BENCH_FLAGS}

.PHONY: bench-remap-functions
bench-remap-functions: ## Run remap-functions benches
	CRITERION_HOME="$(CRITERION_HOME)" cargo bench --manifest-path lib/vrl/stdlib/Cargo.toml ${CARGO_BENCH_FLAGS}

.PHONY: bench-remap
bench-remap: ## Run remap benches
	cargo bench --no-default-features --features "remap-benches" --bench remap ${CARGO_BENCH_FLAGS}

.PHONY: bench-transform
bench-transform: ## Run transform benches
	cargo bench --no-default-features --features "transform-benches" --bench transform ${CARGO_BENCH_FLAGS}

.PHONY: bench-languages
bench-languages:  ### Run language comparison benches
	cargo bench --no-default-features --features "language-benches" --bench languages ${CARGO_BENCH_FLAGS}

.PHONY: bench-metrics
bench-metrics: ## Run metrics benches
	cargo bench --no-default-features --features "metrics-benches" ${CARGO_BENCH_FLAGS}

.PHONY: bench-all
bench-all: ### Run all benches
bench-all: bench-remap-functions
	cargo bench --no-default-features --features "benches remap-benches  metrics-benches language-benches ${DNSTAP_BENCHES}" ${CARGO_BENCH_FLAGS}

##@ Checking

.PHONY: check
check: ## Run prerequisite code checks
	$(VDEV) check rust

.PHONY: check-all
check-all: ## Check everything
check-all: check-fmt check-clippy check-docs
check-all: check-examples check-component-features
check-all: check-scripts check-deny check-generated-docs check-licenses

.PHONY: check-changelog-fragments
check-changelog-fragments: ## Validate changelog fragments added in this branch/PR
	$(VDEV) check changelog-fragments

.PHONY: check-component-features
check-component-features: ## Check that all component features are setup properly
	$(VDEV) check component-features

.PHONY: check-clippy
check-clippy: ## Check code with Clippy
	$(VDEV) check rust

.PHONY: check-docs
check-docs: generate-vrl-docs ## Check that all /docs file are valid - vrl docs due to remap.functions.* references
	$(VDEV) check docs

.PHONY: check-fmt
check-fmt: ## Check that all files are formatted properly
	$(VDEV) check fmt

.PHONY: check-licenses
check-licenses: ## Check that the 3rd-party license file is up to date
	$(VDEV) check licenses

.PHONY: check-markdown
check-markdown: ## Check that markdown is styled properly
	$(VDEV) check markdown

.PHONY: fix-markdown
fix-markdown: ## Auto-fix markdown style issues
	markdownlint-cli2 --fix $(shell git ls-files '*.md')

.PHONY: check-prettier
check-prettier: ## Check that JS/TS/YAML/JSON files are formatted with prettier
	@for ext in yml yaml js ts tsx json; do \
		files=$$(git ls-files "*.$$ext"); \
		if [ -n "$$files" ]; then \
		prettier --ignore-path .prettierignore --check $$files || exit 1; \
		fi; \
	done

.PHONY: fix-prettier
fix-prettier: ## Auto-fix JS/TS/YAML/JSON formatting with prettier
	@for ext in yml yaml js ts tsx json; do \
		files=$$(git ls-files "*.$$ext"); \
		if [ -n "$$files" ]; then \
		prettier --ignore-path .prettierignore --write $$files || exit 1; \
		fi; \
	done

.PHONY: check-examples
check-examples: ## Check that the config/examples files are valid
	$(VDEV) check examples

.PHONY: check-scripts
check-scripts: ## Check that scripts do not have common mistakes
	$(VDEV) check scripts

.PHONY: check-deny
check-deny: ## Check advisories licenses and sources for crate dependencies
	$(VDEV) check deny

.PHONY: check-deny-licenses
check-deny-licenses: ## Check licenses for crate dependencies
	$(VDEV) check deny --licenses-only

.PHONY: check-events
check-events: ## Check that events satisfy patterns set in https://github.com/vectordotdev/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md
	$(VDEV) check events

.PHONY: check-generated-docs
check-generated-docs: generate-docs ## Checks that the machine-generated component Cue docs are up-to-date.
	$(VDEV) check generated-docs

##@ Rustdoc
build-rustdoc: ## Build Vector's Rustdocs
	# This command is mostly intended for use by the build process in vectordotdev/vector-rustdoc
	cargo doc --no-deps --workspace

##@ Packaging (forwarded to Makefile.packaging)

# Packaging targets that depend on VERSION live in Makefile.packaging to avoid
# running `cargo vdev version` when invoking non-packaging targets.

.PHONY: package
package: build ## Build the Vector archive
	$(VDEV) package archive

package-%:
	$(MAKE) -f Makefile.packaging $@

##@ Releasing

.PHONY: release-docker
release-docker: ## Release to Docker Hub
	@$(VDEV) release docker

.PHONY: release-github
release-github: ## Release to GitHub
	@$(VDEV) release github

.PHONY: release-homebrew
release-homebrew: ## Release to vectordotdev Homebrew tap
	@$(VDEV) release homebrew --vector-version $(VECTOR_VERSION)

.PHONY: release-prepare
release-prepare: ## Prepares the release with metadata and highlights
	@$(VDEV) release prepare

.PHONY: release-s3
release-s3: ## Release artifacts to S3
	@$(VDEV) release s3

.PHONY: sha256sum
sha256sum: ## Generate SHA256 checksums of CI artifacts
	scripts/checksum.sh

##@ Vector Remap Language

.PHONY: test-vrl
test-vrl: ## Run the VRL test suite
	@$(VDEV) test-vrl

.PHONY: compile-vrl-wasm
compile-vrl-wasm: ## Compile VRL crates to WASM target
	$(VDEV) build vrl-wasm

##@ Utility

.PHONY: clean
clean: ## Clean everything
	cargo clean

.PHONY: generate-kubernetes-manifests
generate-kubernetes-manifests: ## Generate Kubernetes manifests from latest Helm chart
	$(VDEV) build manifests

.PHONY: generate-component-docs
generate-component-docs: ## Generate per-component Cue docs from the configuration schema.
	cargo build $(if $(findstring true,$(CI)),--quiet,)
	target/debug/vector generate-schema > /tmp/vector-config-schema.json 2>/dev/null
	$(VDEV) build component-docs /tmp/vector-config-schema.json \
		$(if $(findstring true,$(CI)),>/dev/null,)
	./scripts/cue.sh fmt

VRL_DOC_BUILDER := $(shell command -v vector-vrl-doc-builder 2>/dev/null)
ifndef VRL_DOC_BUILDER
VRL_DOC_BUILDER_CMD = cargo run -p vector-vrl-doc-builder --
else
VRL_DOC_BUILDER_CMD = vector-vrl-doc-builder
endif

.PHONY: generate-vector-vrl-docs
generate-vector-vrl-docs: ## Generate VRL function documentation from Rust source.
	$(VRL_DOC_BUILDER_CMD) --output docs/generated/ \
		$(if $(findstring true,$(CI)),>/dev/null,)

.PHONY: generate-vrl-docs
generate-vrl-docs: ## Generate combined VRL function documentation for the website.
	$(MAKE) -C website generate-vrl-docs

.PHONY: generate-docs
generate-docs: generate-component-docs generate-vector-vrl-docs generate-vrl-docs

.PHONY: signoff
signoff: ## Signsoff all previous commits since branch creation
	scripts/signoff.sh

.PHONY: version
version: ## Get the current Vector version
	@$(VDEV) version

.PHONY: git-hooks
git-hooks: ## Add Vector-local git hooks for commit sign-off
	@scripts/install-git-hooks.sh

.PHONY: cargo-install-%
cargo-install-%: override TOOL = $(@:cargo-install-%=%)
cargo-install-%:
	$(if $(findstring true,$(AUTOINSTALL)),cargo install ${TOOL} --quiet; cargo clean,)

.PHONY: ci-generate-publish-metadata
ci-generate-publish-metadata: ## Generates the necessary metadata required for building/publishing Vector.
	$(VDEV) build publish-metadata

.PHONY: clippy-fix
clippy-fix:
	$(VDEV) check rust --fix

.PHONY: fmt
fmt:
	$(VDEV) fmt

.PHONY: build-licenses
build-licenses:
	$(VDEV) build licenses
