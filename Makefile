.PHONY: $(MAKECMDGOALS) all
.DEFAULT_GOAL := help
RUN := $(shell realpath $(shell dirname $(firstword $(MAKEFILE_LIST)))/scripts/run.sh)

# Begin OS detection
ifeq ($(OS),Windows_NT) # is Windows_NT on XP, 2000, 7, Vista, 10...
    export OPERATING_SYSTEM := Windows
	export RUST_TARGET ?= "x86_64-unknown-windows-msvc"
    export DEFAULT_FEATURES = default-msvc
else
    export OPERATING_SYSTEM := $(shell uname)  # same as "uname -s"
	export RUST_TARGET ?= "x86_64-unknown-linux-gnu"
    export DEFAULT_FEATURES = default
endif

# Override this with any scopes for testing/benching.
export SCOPE ?= ""
# Override to false to disable autospawning services on integration tests.
export AUTOSPAWN ?= true
# Override to control if services are turned off after integration tests.
export AUTODESPAWN ?= ${AUTOSPAWN}
# Override to true for a bit more log output in your environment building (more coming!)
export VERBOSE ?= false
# Override to set a different Rust toolchain
export RUST_TOOLCHAIN ?= $(shell cat rust-toolchain)
# Override the container tool.
# TODO: We're working on first class `podman` support for integration tests! We need to move away from compose though: https://github.com/containers/podman-compose/issues/125
export CONTAINER_TOOL ?= docker
# Override this to automatically enter a container containing the correct, full, official build environment for Vector, ready for development
export ENVIRONMENT ?= false
# The upstream container we publish artifacts to on a successful master build.
export ENVIRONMENT_UPSTREAM ?= docker.pkg.github.com/timberio/vector/environment
# Override to disable building the container, having it pull from the Github packages repo instead
# TODO: Disable this by default. Blocked by `docker pull` from Github Packages requiring authenticated login
export ENVIRONMENT_AUTOBUILD ?= true
# Override this when appropriate to disable a TTY being available in commands with `ENVIRONMENT=true` (Useful for CI, but CI uses Nix!)
export ENVIRONMENT_TTY ?= true
# A list of WASM modules by name
export WASM_MODULES = $(patsubst tests/data/wasm/%/,%,$(wildcard tests/data/wasm/*/))
# The same WASM modules, by output path.
export WASM_MODULE_OUTPUTS = $(patsubst %,/target/wasm32-wasi/%,$(WASM_MODULES))

 # Deprecated.
export USE_CONTAINER ?= $(CONTAINER_TOOL)

FORMATTING_BEGIN_YELLOW = \033[0;33m
FORMATTING_BEGIN_BLUE = \033[36m
FORMATTING_END = \033[0m

help:
	@printf -- "${FORMATTING_BEGIN_BLUE}                                      __   __  __${FORMATTING_END}\n"
	@printf -- "${FORMATTING_BEGIN_BLUE}                                      \ \ / / / /${FORMATTING_END}\n"
	@printf -- "${FORMATTING_BEGIN_BLUE}                                       \ V / / / ${FORMATTING_END}\n"
	@printf -- "${FORMATTING_BEGIN_BLUE}                                        \_/  \/  ${FORMATTING_END}\n"
	@printf -- "\n"
	@printf -- "                                      V E C T O R\n"
	@printf -- "\n"
	@printf -- "---------------------------------------------------------------------------------------\n"
	@printf -- "Nix user? You can use ${FORMATTING_BEGIN_YELLOW}\`direnv allow .\`${FORMATTING_END} or ${FORMATTING_BEGIN_YELLOW}\`nix-shell --pure\`${FORMATTING_END}\n"
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
			--network host \
			--mount type=bind,source=${PWD},target=/vector \
			--mount type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock \
			--mount type=volume,source=vector-target,target=/vector/target \
			--mount type=volume,source=vector-cargo-cache,target=/root/.cargo \
			$(ENVIRONMENT_UPSTREAM)
endef

define ENVIRONMENT_COPY_ARTIFACTS
	@echo "Copying artifacts off volumes... (Docker errors below are totally okay)"
	@mkdir -p ./target/release
	@mkdir -p ./target/debug
	@mkdir -p ./target/criterion
	@$(CONTAINER_TOOL) rm -f vector-build-outputs || true
	@$(CONTAINER_TOOL) run \
		-d \
		-v vector-target:/target \
		--name vector-build-outputs \
		busybox true
	@$(CONTAINER_TOOL) cp vector-build-outputs:/target/release/vector ./target/release/ || true
	@$(CONTAINER_TOOL) cp vector-build-outputs:/target/debug/vector ./target/debug/ || true
	@$(CONTAINER_TOOL) cp vector-build-outputs:/target/criterion ./target/criterion || true
	@$(CONTAINER_TOOL) rm -f vector-build-outputs
endef


ifeq ($(ENVIRONMENT_AUTOBUILD), true)
define ENVIRONMENT_PREPARE
	@echo "Building the environment. (ENVIRONMENT_AUTOBUILD=true) This may take a few minutes..."
	$(CONTAINER_TOOL) build \
		$(if $(findstring true,$(VERBOSE)),,--quiet) \
		--tag $(ENVIRONMENT_UPSTREAM) \
		--file scripts/environment/Dockerfile \
		.
endef
else
define ENVIRONMENT_PREPARE
	$(CONTAINER_TOOL) pull $(ENVIRONMENT_UPSTREAM)
endef
endif

environment: export ENVIRONMENT_TTY = true ## Enter a full Vector dev shell in $CONTAINER_TOOL, binding this folder to the container.
environment:
	${ENVIRONMENT_EXEC}

environment-prepare: ## Prepare the Vector dev shell using $CONTAINER_TOOL.
	${ENVIRONMENT_PREPARE}

environment-clean: ## Clean the Vector dev shell using $CONTAINER_TOOL.
	@$(CONTAINER_TOOL) volume rm -f vector-target vector-cargo-cache
	@$(CONTAINER_TOOL) rmi $(ENVIRONMENT_UPSTREAM) || true

environment-push: environment-prepare ## Publish a new version of the container image.
	$(CONTAINER_TOOL) push $(ENVIRONMENT_UPSTREAM)

##@ Building
build: ## Build the project in release mode (Supports `ENVIRONMENT=true`)
	${MAYBE_ENVIRONMENT_EXEC} cargo build --release --no-default-features --features ${DEFAULT_FEATURES}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

build-dev: ## Build the project in development mode (Supports `ENVIRONMENT=true`)
	${MAYBE_ENVIRONMENT_EXEC} cargo build --no-default-features --features ${DEFAULT_FEATURES}

build-all: build-x86_64-unknown-linux-musl build-armv7-unknown-linux-musleabihf build-aarch64-unknown-linux-musl ## Build the project in release mode for all supported platforms

build-x86_64-unknown-linux-gnu: ## Build dynamically linked binary in release mode for the x86_64 architecture
	$(RUN) build-x86_64-unknown-linux-gnu

build-x86_64-unknown-linux-musl: ## Build static binary in release mode for the x86_64 architecture
	$(RUN) build-x86_64-unknown-linux-musl

build-armv7-unknown-linux-musleabihf: load-qemu-binfmt ## Build static binary in release mode for the armv7 architecture
	$(RUN) build-armv7-unknown-linux-musleabihf

build-aarch64-unknown-linux-musl: load-qemu-binfmt ## Build static binary in release mode for the aarch64 architecture
	$(RUN) build-aarch64-unknown-linux-musl

##@ Testing (Supports `ENVIRONMENT=true`)

test: ## Run the test suite
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features ${DEFAULT_FEATURES} ${SCOPE} -- --nocapture

test-all: test-behavior test-integration test-unit ## Runs all tests, unit, behaviorial, and integration.

test-behavior: ## Runs behaviorial test
	${MAYBE_ENVIRONMENT_EXEC} cargo run -- test tests/behavior/**/*.toml

test-integration: ## Runs all integration tests
test-integration: test-integration-aws test-integration-clickhouse test-integration-docker test-integration-elasticsearch
test-integration: test-integration-gcp test-integration-influxdb test-integration-kafka test-integration-loki
test-integration: test-integration-pulsar test-integration-splunk

test-integration-aws: ## Runs AWS integration tests
ifeq ($(AUTOSPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose up -d dependencies-aws
	sleep 5 # Many services are very lazy... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features aws-integration-tests ::aws_ -- --nocapture
ifeq ($(AUTODESPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose stop
endif

test-integration-clickhouse: ## Runs Clickhouse integration tests
ifeq ($(AUTOSPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose up -d dependencies-clickhouse
	sleep 5 # Many services are very lazy... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features clickhouse-integration-tests ::clickhouse:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose stop
endif

test-integration-docker: ## Runs Docker integration tests
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features docker-integration-tests ::docker:: -- --nocapture

test-integration-elasticsearch: ## Runs Elasticsearch integration tests
ifeq ($(AUTOSPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose up -d dependencies-elasticsearch
	sleep 20 # Elasticsearch is incredibly slow to start up, be very generous...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features es-integration-tests ::elasticsearch:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose stop
endif

test-integration-gcp: ## Runs GCP integration tests
ifeq ($(AUTOSPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose up -d dependencies-gcp
	sleep 5 # Many services are very lazy... Give them a sec...
endif
	cargo test --no-default-features --features gcp-integration-tests ::gcp:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose stop
endif

test-integration-influxdb: ## Runs InfluxDB integration tests
ifeq ($(AUTOSPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose up -d dependencies-influxdb
	sleep 5 # Many services are very lazy... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features influxdb-integration-tests ::influxdb::integration_tests:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose stop
endif

test-integration-kafka: ## Runs Kafka integration tests
ifeq ($(AUTOSPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose up -d dependencies-kafka
	sleep 5 # Many services are very lazy... Give them a sec...
endif
	cargo test --no-default-features --features "kafka-integration-tests rdkafka-plain" ::kafka:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose stop
endif

test-integration-loki: ## Runs Loki integration tests
ifeq ($(AUTOSPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose up -d dependencies-loki
	sleep 5 # Many services are very lazy... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features loki-integration-tests ::loki:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose stop
endif

test-integration-pulsar: ## Runs Pulsar integration tests
ifeq ($(AUTOSPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose up -d dependencies-pulsar
	sleep 5 # Many services are very lazy... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features pulsar-integration-tests ::pulsar:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose stop
endif

test-integration-splunk: ## Runs Splunk integration tests
ifeq ($(AUTOSPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose up -d dependencies-splunk
	sleep 5 # Many services are very lazy... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features splunk-integration-tests ::splunk:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose stop
endif

PACKAGE_DEB_USE_CONTAINER ?= "$(USE_CONTAINER)"
test-integration-kubernetes: ## Runs Kubernetes integration tests (Sorry, no `ENVIRONMENT=true` support)
	PACKAGE_DEB_USE_CONTAINER="$(PACKAGE_DEB_USE_CONTAINER)" USE_CONTAINER=none $(RUN) test-integration-kubernetes

test-shutdown: ## Runs shutdown tests
ifeq ($(AUTOSPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose up -d dependencies-kafka
	sleep 5 # Many services are very lazy... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --features shutdown-tests --test shutdown -- --test-threads 4
ifeq ($(AUTODESPAWN), true)
	${MAYBE_ENVIRONMENT_EXEC} $(CONTAINER_TOOL)-compose stop
endif

.PHONY: build-wasm-tests
test-wasm-build-modules: $(WASM_MODULE_OUTPUTS) ### Build all WASM test modules

$(WASM_MODULE_OUTPUTS): MODULE = $(notdir $@)
$(WASM_MODULE_OUTPUTS): ### Build a specific WASM module
	@echo "# Building WASM module ${MODULE}, requires Rustc for wasm32-wasi."
	${MAYBE_ENVIRONMENT_EXEC} cargo build \
		--target-dir target/ \
		--manifest-path tests/data/wasm/${MODULE}/Cargo.toml \
		--target wasm32-wasi \
		--release \
		--package ${MODULE}

.PHONY: test-wasm
test-wasm: export TEST_THREADS=1
test-wasm: export TEST_LOG=vector=trace
test-wasm: $(WASM_MODULE_OUTPUTS)  ### Run engine tests
	${MAYBE_ENVIRONMENT_EXEC} cargo test wasm --no-default-features --features "wasm wasm-timings" -- --nocapture

##@ Benching (Supports `ENVIRONMENT=true`)

bench: ## Run benchmarks in /benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "${DEFAULT_FEATURES}"
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: bench-wasm
bench-wasm: $(WASM_MODULE_OUTPUTS)  ### Run WASM benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "${DEFAULT_FEATURES} transforms-wasm transforms-lua" --bench wasm wasm

##@ Checking

check: ## Run prerequisite code checks
	${MAYBE_ENVIRONMENT_EXEC} cargo check --all --no-default-features --features ${DEFAULT_FEATURES}

check-all: check-fmt check-clippy check-style check-markdown check-generate check-blog check-version check-examples check-component-features check-scripts ## Check everything

check-component-features: ## Check that all component features are setup properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-component-features.sh

check-clippy: ## Check code with Clippy
	${MAYBE_ENVIRONMENT_EXEC} cargo clippy --workspace --all-targets -- -D warnings

check-fmt: ## Check that all files are formatted properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-fmt.sh

check-style: ## Check that all files are styled properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-style.sh

check-markdown: ## Check that markdown is styled properly
	@echo "This requires yarn have been run in the website/ dir!"
	${MAYBE_ENVIRONMENT_EXEC} ./website/node_modules/.bin/markdownlint .

check-generate: ## Check that no files are pending generation
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-generate.sh


check-version: ## Check that Vector's version is correct accounting for recent changes
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-version.rb

check-examples: ## Check that the config/examples files are valid
	${MAYBE_ENVIRONMENT_EXEC} cargo run -- validate --topology --deny-warnings ./config/examples/*.toml

check-scripts: ## Check that scipts do not have common mistakes
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-scripts.sh

##@ Packaging

package-all: package-archive-all package-deb-all package-rpm-all ## Build all packages

package-x86_64-unknown-linux-musl-all: package-archive-x86_64-unknown-linux-musl package-deb-x86_64 package-rpm-x86_64 # Build all x86_64 MUSL packages


package-x86_64-unknown-linux-musl-all: package-archive-x86_64-unknown-linux-musl # Build all x86_64 MUSL packages

package-x86_64-unknown-linux-gnu-all: package-archive-x86_64-unknown-linux-gnu package-deb-x86_64 package-rpm-x86_64 # Build all x86_64 GNU packages

package-armv7-unknown-linux-musleabihf-all: package-archive-armv7-unknown-linux-musleabihf package-deb-armv7 package-rpm-armv7  # Build all armv7 MUSL packages

package-aarch64-unknown-linux-musl-all: package-archive-aarch64-unknown-linux-musl package-deb-aarch64 package-rpm-aarch64  # Build all aarch64 MUSL packages

# archives

package-archive: build ## Build the Vector archive
	$(RUN) package-archive

package-archive-all: package-archive-x86_64-unknown-linux-musl package-archive-x86_64-unknown-linux-gnu package-archive-armv7-unknown-linux-musleabihf package-archive-aarch64-unknown-linux-musl ## Build all archives

package-archive-x86_64-unknown-linux-musl: build-x86_64-unknown-linux-musl ## Build the x86_64 archive
	$(RUN) package-archive-x86_64-unknown-linux-musl

package-archive-x86_64-unknown-linux-gnu: build-x86_64-unknown-linux-gnu ## Build the x86_64 archive
	$(RUN) package-archive-x86_64-unknown-linux-gnu

package-archive-armv7-unknown-linux-musleabihf: build-armv7-unknown-linux-musleabihf ## Build the armv7 archive
	$(RUN) package-archive-armv7-unknown-linux-musleabihf

package-archive-aarch64-unknown-linux-musl: build-aarch64-unknown-linux-musl ## Build the aarch64 archive
	$(RUN) package-archive-aarch64-unknown-linux-musl

# debs

package-deb: ## Build the deb package
	$(RUN) package-deb

package-deb-all: package-deb-x86_64 package-deb-armv7 package-deb-aarch64 ## Build all deb packages

package-deb-x86_64: package-archive-x86_64-unknown-linux-gnu ## Build the x86_64 deb package
	$(RUN) package-deb-x86_64

package-deb-armv7: package-archive-armv7-unknown-linux-musleabihf ## Build the armv7 deb package
	$(RUN) package-deb-armv7

package-deb-aarch64: package-archive-aarch64-unknown-linux-musl  ## Build the aarch64 deb package
	$(RUN) package-deb-aarch64

# rpms

package-rpm: ## Build the rpm package
	$(RUN) package-rpm

package-rpm-all: package-rpm-x86_64 package-rpm-armv7 package-rpm-aarch64 ## Build all rpm packages

package-rpm-x86_64: package-archive-x86_64-unknown-linux-gnu ## Build the x86_64 rpm package
	$(RUN) package-rpm-x86_64

package-rpm-armv7: package-archive-armv7-unknown-linux-musleabihf ## Build the armv7 rpm package
	$(RUN) package-rpm-armv7

package-rpm-aarch64: package-archive-aarch64-unknown-linux-musl ## Build the aarch64 rpm package
	$(RUN) package-rpm-aarch64

##@ Releasing

release: release-prepare generate release-commit ## Release a new Vector version

release-commit: ## Commits release changes
	$(RUN) release-commit

release-docker: ## Release to Docker Hub
	$(RUN) release-docker

release-github: ## Release to Github
	$(RUN) release-github

release-homebrew: ## Release to timberio Homebrew tap
	$(RUN) release-homebrew

release-prepare: ## Prepares the release with metadata and highlights
	@scripts/release-prepare.sh

release-push: ## Push new Vector version
	@scripts/release-push.sh

release-rollback: ## Rollback pending release changes
	@scripts/release-rollback.sh

release-s3: ## Release artifacts to S3
	@scripts/release-s3.sh

sync-install: ## Sync the install.sh script for access via sh.vector.dev
	@aws s3 cp distribution/install.sh s3://sh.vector.dev --sse --acl public-read

##@ Verifying

verify: verify-rpm verify-deb ## Default target, verify all packages

verify-rpm: verify-rpm-amazonlinux-1 verify-rpm-amazonlinux-2 verify-rpm-centos-7 ## Verify all rpm packages

verify-rpm-amazonlinux-1: package-rpm-x86_64 ## Verify the rpm package on Amazon Linux 1
	$(RUN) verify-rpm-amazonlinux-1

verify-rpm-amazonlinux-2: package-rpm-x86_64 ## Verify the rpm package on Amazon Linux 2
	$(RUN) verify-rpm-amazonlinux-2

verify-rpm-centos-7: package-rpm-x86_64 ## Verify the rpm package on CentOS 7
	$(RUN) verify-rpm-centos-7

verify-deb: verify-deb-artifact-on-deb-8 verify-deb-artifact-on-deb-9 verify-deb-artifact-on-deb-10 verify-deb-artifact-on-ubuntu-16-04 verify-deb-artifact-on-ubuntu-18-04 verify-deb-artifact-on-ubuntu-19-04 ## Verify all deb packages

verify-deb-artifact-on-deb-8: package-deb-x86_64 ## Verify the deb package on Debian 8
	$(RUN) verify-deb-artifact-on-deb-8

verify-deb-artifact-on-deb-9: package-deb-x86_64 ## Verify the deb package on Debian 9
	$(RUN) verify-deb-artifact-on-deb-9

verify-deb-artifact-on-deb-10: package-deb-x86_64 ## Verify the deb package on Debian 10
	$(RUN) verify-deb-artifact-on-deb-10

verify-deb-artifact-on-ubuntu-16-04: package-deb-x86_64 ## Verify the deb package on Ubuntu 16.04
	$(RUN) verify-deb-artifact-on-ubuntu-16-04

verify-deb-artifact-on-ubuntu-18-04: package-deb-x86_64 ## Verify the deb package on Ubuntu 18.04
	$(RUN) verify-deb-artifact-on-ubuntu-18-04

verify-deb-artifact-on-ubuntu-19-04: package-deb-x86_64 ## Verify the deb package on Ubuntu 19.04
	$(RUN) verify-deb-artifact-on-ubuntu-19-04

verify-nixos:  ## Verify that Vector can be built on NixOS
	$(RUN) verify-nixos

##@ Website

generate:  ## Generates files across the repo using the data in /.meta
	${MAYBE_ENVIRONMENT_EXEC} bundle exec --gemfile scripts/Gemfile ./scripts/generate.rb

export ARTICLE ?= true
sign-blog: ## Sign newly added blog articles using GPG
	$(RUN) sign-blog

##@ Utility

build-ci-docker-images: ## Rebuilds all Docker images used in CI
	@scripts/build-ci-docker-images.sh

clean: environment-clean ## Clean everything
	cargo clean

fmt: ## Format code
	${MAYBE_ENVIRONMENT_EXEC} cargo fmt
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-style.sh --fix

init-target-dir: ## Create target directory owned by the current user
	$(RUN) init-target-dir

load-qemu-binfmt: ## Load `binfmt-misc` kernel module which required to use `qemu-user`
	$(RUN) load-qemu-binfmt

signoff: ## Signsoff all previous commits since branch creation
	$(RUN) signoff

slim-builds: ## Updates the Cargo config to product disk optimized builds (for CI, not for users)
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/slim-builds.sh

target-graph: ## Display dependencies between targets in this Makefile
	@cd $(shell realpath $(shell dirname $(firstword $(MAKEFILE_LIST)))) && docker-compose run --rm target-graph $(TARGET)

version: ## Get the current Vector version
	$(RUN) version

git-hooks: ## Add Vector-local git hooks for commit sign-off
	@scripts/install-git-hooks.sh

.PHONY: ensure-has-wasm-toolchain ### Configures a wasm toolchain for test artifact building, if required
ensure-has-wasm-toolchain: target/wasm32-wasi/.obtained
target/wasm32-wasi/.obtained:
	@echo "# You should also install WABT for WASM module development!"
	@echo "# You can use your package manager or check https://github.com/WebAssembly/wabt"
	${MAYBE_ENVIRONMENT_EXEC} rustup target add wasm32-wasi
	@mkdir -p target/wasm32-wasi
	@touch target/wasm32-wasi/.obtained

