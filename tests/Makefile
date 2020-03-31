RUN := $(shell realpath $(shell dirname $(firstword $(MAKEFILE_LIST)))/../scripts/docker-compose-run.sh)

# run all tests, checks, and verifications
all: check build-all package-all test-docker test-behavior verify

# default target, build the project in debug mode
build: build-x86_64

# build the project in debug mode for all platforms
build-all: build-x86_64 build-armv7 build-aarch64

build-x86_64:
	$(RUN) build-x86_64

build-armv7: load-qemu-binfmt
	$(RUN) build-armv7

build-aarch64: load-qemu-binfmt
	$(RUN) build-aarch64

# run benchmarks
bench: build
	$(RUN) bench

# run tests which do not require additional services to be present
test: build
	$(RUN) test

# run behaviorial tests
test-behavior: build
	$(RUN) test-behavior

# run tests which use Docker
test-docker: build
	$(RUN) test-docker

# run tests for Kubernetes
test-kubernetes: build-x86_64
	$(RUN) test-kubernetes

# checks

check: check-code check-fmt check-style check-markdown check-generate check-blog check-version check-examples check-component-features

check-code:
	$(RUN) check-code

check-component-features:
	$(RUN) check-component-features

check-fmt:
	$(RUN) check-fmt

check-style:
	$(RUN) check-style

check-markdown:
	$(RUN) check-markdown

check-generate:
	$(RUN) check-generate

check-version:
	$(RUN) check-version

check-examples: build
	$(RUN) check-examples

check-blog:
	$(RUN) check-blog

# generate
generate:
	$(RUN) generate

# packaging

package: build-archive package-deb package-rpm
package-all: build-archive-all package-deb-all package-rpm-all

# .tar.gz archive
build-archive: build-archive-x86_64
build-archive-all: build-archive-x86_64 build-archive-armv7 build-archive-aarch64

build-archive-x86_64: build-x86_64
	$(RUN) build-archive-x86_64

build-archive-armv7: build-armv7
	$(RUN) build-archive-armv7

build-archive-aarch64: build-aarch64
	$(RUN) build-archive-aarch64

# .deb package
package-deb: package-deb-x86_64
package-deb-all: package-deb-x86_64 package-deb-armv7 package-deb-aarch64

package-deb-x86_64: build-archive-x86_64
	$(RUN) package-deb-x86_64

package-deb-armv7: build-archive-armv7
	$(RUN) package-deb-armv7

package-deb-aarch64: build-archive-aarch64
	$(RUN) package-deb-aarch64

# .rpm package
package-rpm: package-rpm-x86_64
package-rpm-all: package-rpm-x86_64 package-rpm-armv7 package-rpm-aarch64

package-rpm-x86_64: build-archive-x86_64
	$(RUN) package-rpm-x86_64

package-rpm-armv7: build-archive-armv7
	$(RUN) package-rpm-armv7

package-rpm-aarch64: build-archive-aarch64
	$(RUN) package-rpm-armv7

# verifications
verify: verify-rpm verify-deb

# verify RPM packages
verify-rpm: verify-rpm-amazonlinux-1 verify-rpm-amazonlinux-2 verify-rpm-centos-7

verify-rpm-amazonlinux-1: package-rpm
	$(RUN) verify-rpm-amazonlinux-1

verify-rpm-amazonlinux-2: package-rpm
	$(RUN) verify-rpm-amazonlinux-2

verify-rpm-centos-7: package-rpm
	$(RUN) verify-rpm-centos-7

# verify DEB packages
verify-deb: verify-deb-artifact-on-deb-8 verify-deb-artifact-on-deb-9 verify-deb-artifact-on-deb-10 verify-deb-artifact-on-ubuntu-16-04 verify-deb-artifact-on-ubuntu-18-04 verify-deb-artifact-on-ubuntu-19-04

verify-deb-artifact-on-deb-8: package-deb
	$(RUN) verify-deb-artifact-on-deb-8

verify-deb-artifact-on-deb-9: package-deb
	$(RUN) verify-deb-artifact-on-deb-9

verify-deb-artifact-on-deb-10: package-deb
	$(RUN) verify-deb-artifact-on-deb-10

verify-deb-artifact-on-ubuntu-16-04: package-deb
	$(RUN) verify-deb-artifact-on-ubuntu-16-04

verify-deb-artifact-on-ubuntu-18-04: package-deb
	$(RUN) verify-deb-artifact-on-ubuntu-18-04

verify-deb-artifact-on-ubuntu-19-04: package-deb
	$(RUN) verify-deb-artifact-on-ubuntu-19-04

# verify that Vector can be built on NixOS
verify-nixos:
	$(RUN) verify-nixos

# miscellaneous

# display dependencies between targets in this Makefile
target-graph:
	@cd $(shell realpath $(shell dirname $(firstword $(MAKEFILE_LIST))))/.. && docker-compose run --rm target-graph $(TARGET)

# clean everything
clean:
	$(RUN) clean

# create target directory owned by the current user
init-target-dir:
	$(RUN) init-target-dir

# load `binfmt-misc` kernel module which required to use `qemu-user`
load-qemu-binfmt:
	$(RUN) load-qemu-binfmt
