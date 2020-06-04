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

export AUTOSPAWN ?= true
export VERBOSE ?= false
export RUST_TOOLCHAIN ?= $(shell cat rust-toolchain)
export CONTAINER_TOOL ?= docker
export ENVIRONMENT ?= false
export ENVIRONMENT_UPSTREAM ?= docker.pkg.github.com/timberio/vector/environment
export ENVIRONMENT_AUTOBUILD ?= true
export ENVIRONMENT_TTY ?= true

# This variables can be used to override the target addresses of unit tests.
# You can override them, just set it before your make call!
export TEST_INTEGRATION_AWS_ADDR ?= $(if $(findstring true,$(INSIDE_ENVIRONMENT)),http://vector_localstack_1:4571,0.0.0.0:6000)
export TEST_INTEGRATION_AWS_CLOUDWATCH_ADDR ?= $(if $(findstring true,$(INSIDE_ENVIRONMENT)),vector_mockwatchlogs_1:6000,0.0.0.0:6000)
export TEST_INTEGRATION_CLICKHOUSE_ADDR ?= $(if $(findstring true,$(INSIDE_ENVIRONMENT)),http://vector_clickhouse_1:8123,http://0.0.0.0:8123)
export TEST_INTEGRATION_ELASTICSEARCH_ADDR_COMM ?= $(if $(findstring true,$(INSIDE_ENVIRONMENT)),http://vector_elasticsearch_1:9200,http://0.0.0.0:9200)
export TEST_INTEGRATION_ELASTICSEARCH_ADDR_HTTP ?= $(if $(findstring true,$(INSIDE_ENVIRONMENT)),http://vector_elasticsearch_1:9300,http://0.0.0.0:9300)
export TEST_INTEGRATION_ELASTICSEARCH_TLS_ADDR_COMM ?= $(if $(findstring true,$(INSIDE_ENVIRONMENT)),https://vector_elasticsearch-tls_1:9200,https://0.0.0.0:9201)
export TEST_INTEGRATION_ELASTICSEARCH_TLS_ADDR_HTTP ?= $(if $(findstring true,$(INSIDE_ENVIRONMENT)),https://vector_elasticsearch-tls_1:9300,https://0.0.0.0:9301)

 # Deprecated.
export USE_CONTAINER ?= $(CONTAINER_TOOL)


help:
	@echo "                                      __   __  __"
	@echo "                                      \ \ / / / /"
	@echo "                                       \ V / / / "
	@echo "                                        \_/  \/  "
	@echo ""
	@echo "                                      V E C T O R"
	@echo ""
	@echo "---------------------------------------------------------------------------------------"
	@echo ""
	@awk 'BEGIN {FS = ":.*##"; printf "Usage: make \033[36m<target>\033[0m\n"} /^[a-zA-Z0-9_-]+:.*?##/ { printf "  \033[36m%-46s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Environment (Nix users use `nix-shell` instead)
# We use a volume here as non-Linux hosts are extremely slow to share disks, and Linux hosts tend to get permissions clobbered.
define ENVIRONMENT_EXEC
	@echo "Entering environment..."
	@mkdir -p target
	@$(CONTAINER_TOOL) network create environment || true
	$(CONTAINER_TOOL) run \
			--name vector-environment \
			--rm \
			$(if $(findstring true,$(ENVIRONMENT_TTY)),--tty,) \
			--init \
			--interactive \
			--env INSIDE_ENVIRONMENT=true \
			--network environment \
			--dns-search environment \
			--hostname vector \
			--mount type=bind,source=${PWD},target=/vector \
			--mount type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock \
			--mount type=volume,source=vector-target,target=/vector/target \
			--mount type=volume,source=vector-cargo-cache,target=/root/.cargo \
			--publish 3000:3000 \
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
	@$(CONTAINER_TOOL) network create environment || true
endef
else
define ENVIRONMENT_PREPARE
	$(CONTAINER_TOOL) pull $(ENVIRONMENT_UPSTREAM)
endef
endif


environment: ## Enter a full Vector dev shell in Docker, binding this folder to the container.
	${ENVIRONMENT_PREPARE}
	@export ENVIRONMENT_TTY=true
	${ENVIRONMENT_EXEC}

environment-prepare: ## Prepare the Vector dev env.
	${ENVIRONMENT_PREPARE}

environment-clean: ## Clean the Vector dev env.
	@$(CONTAINER_TOOL) volume rm -f vector-target vector-cargo-cache
	@$(CONTAINER_TOOL) rmi $(ENVIRONMENT_UPSTREAM)

environment-push: environment-prepare ## Publish a new version of the docker image.
	$(CONTAINER_TOOL) push $(ENVIRONMENT_UPSTREAM)

##@ Building
build: ## Build the project in release mode (Use `ENVIRONMENT=true` to run in a container)
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make build
	${ENVIRONMENT_COPY_ARTIFACTS}
else
	cargo build --release --no-default-features --features ${DEFAULT_FEATURES}
endif

build-dev: ## Build the project in development mode (Use `ENVIRONMENT=true` to run in a container)
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make build-dev
	${ENVIRONMENT_COPY_ARTIFACTS}
else
	cargo build --no-default-features --features ${DEFAULT_FEATURES}
endif

build-all: build-x86_64-unknown-linux-musl build-armv7-unknown-linux-musleabihf build-aarch64-unknown-linux-musl ## Build the project in release mode for all supported platforms

build-x86_64-unknown-linux-gnu: ## Build dynamically linked binary in release mode for the x86_64 architecture
	$(RUN) build-x86_64-unknown-linux-gnu

build-x86_64-unknown-linux-musl: ## Build static binary in release mode for the x86_64 architecture
	$(RUN) build-x86_64-unknown-linux-musl

build-armv7-unknown-linux-musleabihf: load-qemu-binfmt ## Build static binary in release mode for the armv7 architecture
	$(RUN) build-armv7-unknown-linux-musleabihf

build-aarch64-unknown-linux-musl: load-qemu-binfmt ## Build static binary in release mode for the aarch64 architecture
	$(RUN) build-aarch64-unknown-linux-musl

##@ Testing

test: ## Run the test suite (Use `ENVIRONMENT=true` to run in a container)
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make test
	${ENVIRONMENT_COPY_ARTIFACTS}
else
	cargo test --no-default-features --features ${DEFAULT_FEATURES}
endif

test-all: test-behavior test-integration test-unit ## Runs all tests, unit, behaviorial, and integration.

test-behavior: ## Runs behaviorial tests
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make test-behavior
	${ENVIRONMENT_COPY_ARTIFACTS}
else
	cargo run -- test tests/behavior/**/*.toml
endif

test-integration: ## Runs all integration tests
test-integration: test-integration-aws test-integration-clickhouse test-integration-docker test-integration-elasticsearch
test-integration: test-integration-gcp test-integration-influxdb test-integration-kafka test-integration-loki
test-integration: test-integration-pulsar test-integration-splunk

test-integration-aws: ## Runs Clickhouse integration tests
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make test-integration-aws
else
	docker-compose up -d dependencies-aws
	export TEST_LOG="vector=debug"
	export RUST_TEST_THREADS=1
	cargo test --no-default-features --features aws-integration-tests ::aws_cloudwatch_logs:: -- --nocapture
	cargo test --no-default-features --features aws-integration-tests ::aws_cloudwatch_metrics:: -- --nocapture
	cargo test --no-default-features --features aws-integration-tests ::aws_kinesis_firehose:: -- --nocapture
	cargo test --no-default-features --features aws-integration-tests ::aws_kinesis_streams:: -- --nocapture
	cargo test --no-default-features --features aws-integration-tests ::aws_s3:: -- --nocapture
endif

test-integration-clickhouse: ## Runs Clickhouse integration testsbv
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make test-integration-clickhouse
else
	docker-compose up -d dependencies-clickhouse
	export TEST_LOG="vector=debug"
	export RUST_TEST_THREADS=1
	cargo test --no-default-features --features clickhouse-integration-tests ::clickhouse:: -- --nocapture
endif

test-integration-docker: ## Runs Docker integration tests
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make test-integration-docker
else
	cargo test --no-default-features --features docker-integration-tests ::docker:: -- --nocapture
endif

test-integration-elasticsearch: ## Runs Elasticsearch integration tests
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make test-integration-elasticsearch
else
	if $(AUTOSPAWN); then \
		docker-compose up -d dependencies-elasticsearch; \
	fi
	# export TEST_LOG="vector=debug"
	# export RUST_TEST_THREADS=1
	cargo test --no-default-features --features es-integration-tests ::elasticsearch:: -- --nocapture
endif

test-integration-gcp: ## Runs GCP integration tests
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make test-integration-gcp
else
	if $(AUTOSPAWN); then \
		docker-compose up -d dependencies-gcp; \
	fi
	export TEST_LOG="vector=debug"
	export RUST_TEST_THREADS=1
	cargo test --no-default-features --features gcp-integration-tests ::gcp:: -- --nocapture
endif

test-integration-influxdb: ## Runs Kafka integration tests
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make test-integration-influxdb
else
	if $(AUTOSPAWN); then \
		docker-compose up -d dependencies-influxdb; \
	fi
	export TEST_LOG="vector=debug"
	export RUST_TEST_THREADS=1
	cargo test --no-default-features --features influxdb-integration-tests ::influxdb::integration_tests:: -- --nocapture
endif

test-integration-kafka: ## Runs Kafka integration tests
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make test-integration-kafka
else
	if $(AUTOSPAWN); then \
		docker-compose up -d dependencies-kafka; \
	fi
	export TEST_LOG="vector=debug"
	export RUST_TEST_THREADS=1
	cargo test --no-default-features --features kafka-integration-tests ::kafka:: -- --nocapture
endif

test-integration-loki: ## Runs Loki integration tests (Use `ENVIRONMENT=true` to run in a container)
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make test-integration-loki
else
	if $(AUTOSPAWN); then \
		docker-compose up -d dependencies-loki; \
	fi
	export TEST_LOG="vector=debug"
	export RUST_TEST_THREADS=1
	cargo test --no-default-features --features loki-integration-tests ::loki:: -- --nocapture
endif

test-integration-pulsar: ## Runs Pulsar integration tests
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make test-integration-pulsar
else
	if $(AUTOSPAWN); then \
		docker-compose up -d dependencies-pulsar; \
	fi
	cargo test --no-default-features --features pulsar-integration-tests ::pulsar:: -- --nocapture
endif

test-integration-splunk: ## Runs Splunk integration tests
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make test-integration-splunk
else
	if $(AUTOSPAWN); then \
		docker-compose up -d dependencies-splunk; \
	fi
	cargo test --no-default-features --features splunk-integration-tests ::splunk:: -- --nocapture
endif

PACKAGE_DEB_USE_CONTAINER ?= "$(USE_CONTAINER)"
test-integration-kubernetes: ## Runs Kubernetes integration tests
	PACKAGE_DEB_USE_CONTAINER="$(PACKAGE_DEB_USE_CONTAINER)" USE_CONTAINER=none $(RUN) test-integration-kubernetes

test-shutdown: ## Runs shutdown tests
	$(RUN) test-shutdown

##@ Benching

bench: ## Run benchmarks in /benches (Use `ENVIRONMENT=true` to run in a container)
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make bench
	${ENVIRONMENT_COPY_ARTIFACTS}
else
	cargo bench --no-default-features --features ${DEFAULT_FEATURES}
endif

##@ Checking

check: ## Run prerequisite code checks (Use `ENVIRONMENT=true` to run in a container)
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make check
else
	cargo check --all --no-default-features --features ${DEFAULT_FEATURES}
endif

check-all: check-fmt check-style check-markdown check-generate check-blog check-version check-examples check-component-features check-scripts ## Check everything

check-component-features: ## Check that all component features are setup properly
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make check-component-features
else
	./scripts/check-component-features.sh
endif

check-fmt: ## Check that all files are formatted properly
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make check-fmt
else
	./scripts/check-fmt.sh
endif

check-style: ## Check that all files are styled properly
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make check-style
else
	./scripts/check-style.sh
endif

check-markdown: ## Check that markdown is styled properly
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make check-markdown
else
	# TODO: Install markdownlint
	markdownlint .
endif

check-generate: ## Check that no files are pending generation
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make check-generate
else
	./scripts/check-generate.sh
endif


check-version: ## Check that Vector's version is correct accounting for recent changes
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make check-version
else
	./scripts/check-version.rb
endif

check-examples: ## Check that the config/exmaples files are valid
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make check-examples
else
	cargo run -- validate --topology --deny-warnings ./config/examples/*.toml
endif

check-scripts: ## Check that scipts do not have common mistakes
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make check-scripts
else
	./scripts/check-scripts.sh
endif

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
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make generate
else
	bundle exec --gemfile scripts/Gemfile ./scripts/generate.rb
endif

export ARTICLE ?= true
sign-blog: ## Sign newly added blog articles using GPG
	$(RUN) sign-blog

##@ Utility

build-ci-docker-images: ## Rebuilds all Docker images used in CI
	@scripts/build-ci-docker-images.sh

clean: environment-clean ## Clean everything
	cargo clean

fmt: check-style ## Format code
ifeq ($(ENVIRONMENT), true)
	${ENVIRONMENT_PREPARE}
	${ENVIRONMENT_EXEC} make fmt
else
	cargo fmt
endif

init-target-dir: ## Create target directory owned by the current user
	$(RUN) init-target-dir

load-qemu-binfmt: ## Load `binfmt-misc` kernel module which required to use `qemu-user`
	$(RUN) load-qemu-binfmt

signoff: ## Signsoff all previous commits since branch creation
	$(RUN) signoff

slim-builds: ## Updates the Cargo config to product disk optimized builds, useful for CI
	$(RUN) slim-builds

target-graph: ## Display dependencies between targets in this Makefile
	@cd $(shell realpath $(shell dirname $(firstword $(MAKEFILE_LIST)))) && docker-compose run --rm target-graph $(TARGET)

version: ## Get the current Vector version
	$(RUN) version

git-hooks: ## Add Vector-local git hooks for commit sign-off
	@scripts/install-git-hooks.sh
