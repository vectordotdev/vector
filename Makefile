.PHONY: help
.DEFAULT_GOAL := help
RUN := $(shell realpath $(shell dirname $(firstword $(MAKEFILE_LIST)))/scripts/run.sh)

export USE_CONTAINER ?= docker

# Begin OS detection
ifeq ($(OS),Windows_NT) # is Windows_NT on XP, 2000, 7, Vista, 10...
    export OPERATING_SYSTEM := Windows
    export DEFAULT_FEATURES = default-msvc
else
    export OPERATING_SYSTEM := $(shell uname)  # same as "uname -s"
    export DEFAULT_FEATURES = default
endif

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

##@ Default

all: check build-all package-all test-docker test-behavior verify ## Run all tests, checks, and verifications

##@ Building

build: ## Build the project natively in release mode
	$(RUN) build

build-all: build-x86_64-unknown-linux-musl build-armv7-unknown-linux-musleabihf build-aarch64-unknown-linux-musl ## Build the project in release mode for all supported platforms

build-x86_64-unknown-linux-musl: ## Build the project in release mode for the x86_64 architecture
	$(RUN) build-x86_64-unknown-linux-musl

build-armv7-unknown-linux-musleabihf: load-qemu-binfmt ## Build the project in release mode for the armv7 architecture
	$(RUN) build-armv7-unknown-linux-musleabihf

build-aarch64-unknown-linux-musl: load-qemu-binfmt ## Build the project in release mode for the aarch64 architecture
	$(RUN) build-aarch64-unknown-linux-musl

##@ Developing

bench: build ## Run benchmarks in /benches
	$(RUN) bench

test: test-behavior test-integration test-unit ## Runs all tests, unit, behaviorial, and integration.

test-behavior: build ## Runs behaviorial tests
	$(RUN) test-behavior

test-integration: ## Runs all integration tests
	$(RUN) test-integration

test-integration-aws: ## Runs Clickhouse integration tests
	$(RUN) test-integration-aws

test-integration-clickhouse: ## Runs Clickhouse integration tests
	$(RUN) test-integration-clickhouse

test-integration-docker: ## Runs Docker integration tests
	$(RUN) test-integration-docker

test-integration-elasticsearch: ## Runs Elasticsearch integration tests
	$(RUN) test-integration-elasticsearch

test-integration-gcp: ## Runs GCP integration tests
	$(RUN) test-integration-gcp

test-integration-influxdb: ## Runs Kafka integration tests
	$(RUN) test-integration-influxdb

test-integration-kafka: ## Runs Kafka integration tests
	$(RUN) test-integration-kafka

test-integration-loki: ## Runs Loki integration tests
	$(RUN) test-integration-loki

test-integration-pulsar: ## Runs Kafka integration tests
	$(RUN) test-integration-pulsar

test-integration-splunk: ## Runs Kafka integration tests
	$(RUN) test-integration-splunk

PACKAGE_DEB_USE_CONTAINER ?= "$(USE_CONTAINER)"
test-integration-kubernetes: ## Runs Kubernetes integration tests
	PACKAGE_DEB_USE_CONTAINER="$(PACKAGE_DEB_USE_CONTAINER)" USE_CONTAINER=none $(RUN) test-integration-kubernetes

test-unit: ## Runs unit tests, tests which do not require additional services to be present
	$(RUN) test-unit

##@ Checking

check: check-all ## Default target, check everything

check-all: check-code check-fmt check-style check-markdown check-generate check-blog check-version check-examples check-component-features check-scripts ## Check everything

check-code: ## Check code
	$(RUN) check-code

check-component-features: ## Check that all component features are setup properly
	$(RUN) check-component-features

check-fmt: ## Check that all files are formatted properly
	$(RUN) check-fmt

check-style: ## Check that all files are styled properly
	$(RUN) check-style

check-markdown: ## Check that markdown is styled properly
	$(RUN) check-markdown

check-generate: ## Check that no files are pending generation
	$(RUN) check-generate

check-version: ## Check that Vector's version is correct accounting for recent changes
	$(RUN) check-version

check-examples: build ## Check that the config/exmaples files are valid
	$(RUN) check-examples

check-blog: ## Check that all blog posts are signed and valid
	$(RUN) check-blog

check-scripts: ## Check that scipts do not have common mistakes
	$(RUN) check-scripts

##@ Packaging

package-all: package-archive-all package-deb-all package-rpm-all ## Build all packages

package-x86_64-unknown-linux-musl-all: package-archive-x86_64-unknown-linux-musl package-deb-x86_64 package-rpm-x86_64 # Build all x86_64 MUSL packages

package-armv7-unknown-linux-musleabihf-all: package-archive-armv7-unknown-linux-musleabihf package-deb-armv7 package-rpm-armv7  # Build all armv7 MUSL packages

package-aarch64-unknown-linux-musl-all: package-archive-aarch64-unknown-linux-musl package-deb-aarch64 package-rpm-aarch64  # Build all aarch64 MUSL packages

# archives

package-archive: build ## Build the Vector archive
	$(RUN) package-archive

package-archive-all: package-archive-x86_64-unknown-linux-musl package-archive-armv7-unknown-linux-musleabihf package-archive-aarch64-unknown-linux-musl ## Build all archives

package-archive-x86_64-unknown-linux-musl: build-x86_64-unknown-linux-musl ## Build the x86_64 archive
	$(RUN) package-archive-x86_64-unknown-linux-musl

package-archive-armv7-unknown-linux-musleabihf: build-armv7-unknown-linux-musleabihf ## Build the armv7 archive
	$(RUN) package-archive-armv7-unknown-linux-musleabihf

package-archive-aarch64-unknown-linux-musl: build-aarch64-unknown-linux-musl ## Build the aarch64 archive
	$(RUN) package-archive-aarch64-unknown-linux-musl

# debs

package-deb: ## Build the deb package
	$(RUN) package-deb

package-deb-all: package-deb-x86_64 package-deb-armv7 package-deb-aarch64 ## Build all deb packages

package-deb-x86_64: package-archive-x86_64-unknown-linux-musl ## Build the x86_64 deb package
	$(RUN) package-deb-x86_64

package-deb-armv7: package-archive-armv7-unknown-linux-musleabihf ## Build the armv7 deb package
	$(RUN) package-deb-armv7

package-deb-aarch64: package-archive-aarch64-unknown-linux-musl  ## Build the aarch64 deb package
	$(RUN) package-deb-aarch64

# rpms

package-rpm: ## Build the rpm package
	$(RUN) package-rpm

package-rpm-all: package-rpm-x86_64 package-rpm-armv7 package-rpm-aarch64 ## Build all rpm packages

package-rpm-x86_64: package-archive-x86_64-unknown-linux-musl ## Build the x86_64 rpm package
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
	$(RUN) generate

export ARTICLE ?= true
sign-blog: ## Sign newly added blog articles using GPG
	$(RUN) sign-blog

##@ Utility

build-ci-docker-images: ## Rebuilds all Docker images used in CI
	@scripts/build-ci-docker-images.sh

clean: ## Clean everything
	$(RUN) clean

fmt: ## Format code
	$(RUN) fmt

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
