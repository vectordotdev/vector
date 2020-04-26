.PHONY: help
.DEFAULT_GOAL := help
RUN := $(shell realpath $(shell dirname $(firstword $(MAKEFILE_LIST)))/../scripts/run2.sh)

export USE_CONTAINER ?= docker

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
	@awk 'BEGIN {FS = ":.*##"; printf "Usage: make \033[36m<target>\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-25s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Default

all: check build-all package-all test-docker test-behavior verify ## run all tests, checks, and verifications

##@ Building

build: build-x86_64-unknown-linux-musl ## default target, build the project in debug mode

build-all: build-x86_64-unknown-linux-musl build-armv7-unknown-linux-musleabihf build-aarch64-unknown-linux-musl ## build the project in debug mode for all platforms

build-x86_64-unknown-linux-musl: ## build the project for the x86_64 architecture
	$(RUN) build-x86_64-unknown-linux-musl

build-armv7-unknown-linux-musleabihf: load-qemu-binfmt ## build the project for the armv7 architecture
	$(RUN) build-armv7-unknown-linux-musleabihf

build-aarch64-unknown-linux-musl: load-qemu-binfmt ## build the project for the aarch64 architecture
	$(RUN) build-aarch64-unknown-linux-musl

##@ Developing

bench: build ## run benchmarks
	$(RUN) bench

test: build ## run tests which do not require additional services to be present
	$(RUN) test

test-behavior: build ## run behaviorial tests
	$(RUN) test-behavior

test-integration: build ## run tests which use Docker
	$(RUN) test-integration

test-kubernetes: build-x86_64-unknown-linux-musl ## run tests for Kubernetes
	$(RUN) test-kubernetes

##@ Checking

check: check-all ## default target, check everything

check-all: check-code check-fmt check-style check-markdown check-generate check-blog check-version check-examples check-component-features ## check everything

check-code: ## check code
	$(RUN) check-code

check-component-features: ## check that all component features are setup properly
	$(RUN) check-component-features

check-fmt: ## check that all files are formatted properly
	$(RUN) check-fmt

check-style: ## check that all files are styled properly
	$(RUN) check-style

check-markdown: ## check that markdown is styled properly
	$(RUN) check-markdown

check-generate: ## check that no files are pending generation
	$(RUN) check-generate

check-version: ## check that Vector's version is correct accounting for recent changes
	$(RUN) check-version

check-examples: build ## check that the config/exmaples files are valid
	$(RUN) check-examples

check-blog: ## check that all blog posts are signed and valid
	$(RUN) check-blog

##@ Packaging

package: build-archive package-deb package-rpm ## default target, package x86_84

package-all: build-archive-all package-deb-all package-rpm-all ## package everything

build-archive: build-archive-x86_64-unknown-linux-musl ## default target, build the x86_64 archives

build-archive-all: build-archive-x86_64-unknown-linux-musl build-archive-armv7-unknown-linux-musleabihf build-archive-aarch64-unknown-linux-musl ## build all archives

build-archive-x86_64-unknown-linux-musl: build-x86_64-unknown-linux-musl ## build the x86_64 archive
	$(RUN) build-archive-x86_64-unknown-linux-musl

build-archive-armv7-unknown-linux-musleabihf: build-armv7-unknown-linux-musleabihf ## build the armv7 archive
	$(RUN) build-archive-armv7-unknown-linux-musleabihf

build-archive-aarch6-unknown-linux-musl: build-aarch64-unknown-linux-musl ## build the aarch64 archive
	$(RUN) build-archive-aarch64-unknown-linux-musl

package-deb: package-deb-x86_64 ## default target, build the x86_64 deb package

package-deb-all: package-deb-x86_64 package-deb-armv7 package-deb-aarch64 ## build all deb packages

package-deb-x86_64: build-archive-x86_64 ## build the x86_64 deb package
	$(RUN) package-deb-x86_64

package-deb-armv7: build-archive-armv7 ## build the armv7 deb package
	$(RUN) package-deb-armv7

package-deb-aarch64: build-archive-aarch64  ## build the aarch64 deb package
	$(RUN) package-deb-aarch64

package-rpm: package-rpm-x86_64 ## default target, build the x86_64 rpm package

package-rpm-all: package-rpm-x86_64 package-rpm-armv7 package-rpm-aarch64 ## build all rpm packages

package-rpm-x86_64: build-archive-x86_64 ## build the x86_64 rpm package
	$(RUN) package-rpm-x86_64

package-rpm-armv7: build-archive-armv7 ## build the armv7 rpm package
	$(RUN) package-rpm-armv7

package-rpm-aarch64: build-archive-aarch64 ## build the aarch64 rpm package
	$(RUN) package-rpm-aarch64

##@ Verifying

verify: verify-rpm verify-deb ## default target, verify all packages

verify-rpm: verify-rpm-amazonlinux-1 verify-rpm-amazonlinux-2 verify-rpm-centos-7 ## verify all rpm packages

verify-rpm-amazonlinux-1: package-rpm ## verify the rpm package on Amazon Linux 1
	$(RUN) verify-rpm-amazonlinux-1

verify-rpm-amazonlinux-2: package-rpm ## verify the rpm package on Amazon Linux 2
	$(RUN) verify-rpm-amazonlinux-2

verify-rpm-centos-7: package-rpm ## verify the rpm package on CentOS 7
	$(RUN) verify-rpm-centos-7

verify-deb: verify-deb-artifact-on-deb-8 verify-deb-artifact-on-deb-9 verify-deb-artifact-on-deb-10 verify-deb-artifact-on-ubuntu-16-04 verify-deb-artifact-on-ubuntu-18-04 verify-deb-artifact-on-ubuntu-19-04 ## verify all deb packages

verify-deb-artifact-on-deb-8: package-deb ## verify the deb package on Debian 8
	$(RUN) verify-deb-artifact-on-deb-8

verify-deb-artifact-on-deb-9: package-deb ## verify the deb package on Debian 9
	$(RUN) verify-deb-artifact-on-deb-9

verify-deb-artifact-on-deb-10: package-deb ## verify the deb package on Debian 10
	$(RUN) verify-deb-artifact-on-deb-10

verify-deb-artifact-on-ubuntu-16-04: package-deb ## verify the deb package on Ubuntu 16.04
	$(RUN) verify-deb-artifact-on-ubuntu-16-04

verify-deb-artifact-on-ubuntu-18-04: package-deb ## verify the deb package on Ubuntu 18.04
	$(RUN) verify-deb-artifact-on-ubuntu-18-04

verify-deb-artifact-on-ubuntu-19-04: package-deb ## verify the deb package on Ubuntu 19.04
	$(RUN) verify-deb-artifact-on-ubuntu-19-04

verify-nixos:  ## verify that Vector can be built on NixOS
	$(RUN) verify-nixos

##@ Website

generate:  ## Generates files across the repo using the data in /.meta
	$(RUN) generate

##@ Utility

target-graph: ## display dependencies between targets in this Makefile
	@cd $(shell realpath $(shell dirname $(firstword $(MAKEFILE_LIST))))/.. && docker-compose run --rm target-graph $(TARGET)

clean: ## clean everything
	$(RUN) clean

init-target-dir: ## create target directory owned by the current user
	$(RUN) init-target-dir

load-qemu-binfmt: ## load `binfmt-misc` kernel module which required to use `qemu-user`
	$(RUN) load-qemu-binfmt
