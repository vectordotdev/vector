RUN := $(shell realpath $(shell dirname $(firstword $(MAKEFILE_LIST)))/../scripts/docker-compose-run.sh)

# default target, build the project in debug mode
build: build-x86_64

# build the project in debug mode for all platforms
build-all: build-x86_64 build-armv7 build-aarch64

build-x86_64: init-target-dir
	$(RUN) build-x86_64

build-armv7: init-target-dir load-qemu-binfmt
	$(RUN) build-armv7

build-aarch64: init-target-dir load-qemu-binfmt
	$(RUN) build-aarch64

# run benchmarks
bench: build
	$(RUN) bench

# run tests which do not require additional services to be present
test: build
	$(RUN) test

# run tests which use Docker
test-docker: build
	$(RUN) test-docker

# checks

check: check-code check-fmt check-style check-generate check-blog check-version check-examples

check-code: init-target-dir
	$(RUN) check-code

check-fmt: init-target-dir
	$(RUN) check-fmt

check-style:
	$(RUN) check-style

check-generate:
	$(RUN) check-generate

check-version:
	$(RUN) check-version

check-examples: build-archive
	$(RUN) check-examples

check-blog:
	$(RUN) check-blog

# packaging

build-archive: init-target-dir
	$(RUN) build-archive

package-deb: build-archive
	$(RUN) package-deb

package-rpm: build-archive
	$(RUN) package-rpm

# verifications
verify: verify-rpm verify-deb verify-nixos

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
