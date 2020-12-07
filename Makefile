# .PHONY: $(MAKECMDGOALS) all
.DEFAULT_GOAL := help

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
# Override autoinstalling of tools. (Eg `cargo install`)
export AUTOINSTALL ?= false
# Override to true for a bit more log output in your environment building (more coming!)
export VERBOSE ?= false
# Override to set a different Rust toolchain
export RUST_TOOLCHAIN ?= $(shell cat rust-toolchain)
# Override the container tool. Tries docker first and then tries podman.
export CONTAINER_TOOL ?= auto
ifeq ($(CONTAINER_TOOL),auto)
	override CONTAINER_TOOL = $(shell docker version >/dev/null 2>&1 && echo docker || echo podman)
endif
# If we're using podman create pods else if we're using docker create networks.
ifeq ($(CONTAINER_TOOL),podman)
    export CONTAINER_ENCLOSURE = "pod"
else
    export CONTAINER_ENCLOSURE = "network"
endif
export CURRENT_DIR = $(shell pwd)

# Override this to automatically enter a container containing the correct, full, official build environment for Vector, ready for development
export ENVIRONMENT ?= false
# The upstream container we publish artifacts to on a successful master build.
export ENVIRONMENT_UPSTREAM ?= timberio/ci_image
# Override to disable building the container, having it pull from the Github packages repo instead
# TODO: Disable this by default. Blocked by `docker pull` from Github Packages requiring authenticated login
export ENVIRONMENT_AUTOBUILD ?= true
# Override this when appropriate to disable a TTY being available in commands with `ENVIRONMENT=true`
export ENVIRONMENT_TTY ?= true
# A list of WASM modules by name
export WASM_MODULES = $(patsubst tests/data/wasm/%/,%,$(wildcard tests/data/wasm/*/))
# The same WASM modules, by output path.
export WASM_MODULE_OUTPUTS = $(patsubst %,/target/wasm32-wasi/%,$(WASM_MODULES))

# Set dummy AWS credentials if not present - used for AWS and ES integration tests
export AWS_ACCESS_KEY_ID ?= "dummy"
export AWS_SECRET_ACCESS_KEY ?= "dummy"

# Set if you are on the CI and actually want the things to happen. (Non-CI users should never set this.)
export CI ?= false

FORMATTING_BEGIN_YELLOW = \033[0;33m
FORMATTING_BEGIN_BLUE = \033[36m
FORMATTING_END = \033[0m

# "One weird trick!" https://www.gnu.org/software/make/manual/make.html#Syntax-of-Functions
EMPTY:=
SPACE:= ${EMPTY} ${EMPTY}

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
			--network host \
			--mount type=bind,source=${CURRENT_DIR},target=/git/timberio/vector \
			--mount type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock \
			--mount type=volume,source=vector-target,target=/git/timberio/vector/target \
			--mount type=volume,source=vector-cargo-cache,target=/root/.cargo \
			$(ENVIRONMENT_UPSTREAM)
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
	@$(CONTAINER_TOOL) volume rm -f vector-target vector-cargo-cache
	@$(CONTAINER_TOOL) rmi $(ENVIRONMENT_UPSTREAM) || true

.PHONY: environment-push
environment-push: environment-prepare ## Publish a new version of the container image.
	$(CONTAINER_TOOL) push $(ENVIRONMENT_UPSTREAM)

# All release tasks are contained in the release.mk file
include make/release.mk

# All test tasks are contained in the test.mk file
include make/test.mk

# All bench marking tasks are contained in the bench.mk file
include make/bench.mk

# All check tasks are contained in the check.mk file
include make/check.mk

# All utility tasks are contained in the util.mk
include make/util.mk

##@ Building
.PHONY: build
build: ## Build the project in release mode (Supports `ENVIRONMENT=true`)
	${MAYBE_ENVIRONMENT_EXEC} cargo build --release --no-default-features --features ${DEFAULT_FEATURES}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: build-dev
build-dev: ## Build the project in development mode (Supports `ENVIRONMENT=true`)
	${MAYBE_ENVIRONMENT_EXEC} cargo build --no-default-features --features ${DEFAULT_FEATURES}

.PHONY: build-x86_64-unknown-linux-gnu
build-x86_64-unknown-linux-gnu: target/x86_64-unknown-linux-gnu/release/vector ## Build a release binary for the x86_64-unknown-linux-gnu triple.
	@echo "Output to ${<}"

.PHONY: build-aarch64-unknown-linux-gnu
build-aarch64-unknown-linux-gnu: target/aarch64-unknown-linux-gnu/release/vector ## Build a release binary for the aarch64-unknown-linux-gnu triple.
	@echo "Output to ${<}"

.PHONY: build-x86_64-unknown-linux-musl
build-x86_64-unknown-linux-musl: target/x86_64-unknown-linux-musl/release/vector ## Build a release binary for the aarch64-unknown-linux-gnu triple.
	@echo "Output to ${<}"

.PHONY: build-aarch64-unknown-linux-musl
build-aarch64-unknown-linux-musl: target/aarch64-unknown-linux-musl/release/vector ## Build a release binary for the aarch64-unknown-linux-gnu triple.
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

##@ Cross Compiling
.PHONY: cross-enable
cross-enable: cargo-install-cross ## Install Cross

.PHONY: CARGO_HANDLES_FRESHNESS
CARGO_HANDLES_FRESHNESS:
	${EMPTY}

# This is basically a shorthand for folks.
# `cross-anything-triple` will call `cross anything --target triple` with the right features.
.PHONY: cross-%
cross-%: export PAIR =$(subst -, ,$($(strip @):cross-%=%))
cross-%: export COMMAND ?=$(word 1,${PAIR})
cross-%: export TRIPLE ?=$(subst ${SPACE},-,$(wordlist 2,99,${PAIR}))
cross-%: export PROFILE ?= release
cross-%: export RUSTFLAGS += -C link-arg=-s
cross-%: cargo-install-cross
	$(MAKE) -k cross-image-${TRIPLE}
	cross ${COMMAND} \
		$(if $(findstring release,$(PROFILE)),--release,) \
		--target ${TRIPLE} \
		--no-default-features \
		--features target-${TRIPLE}

target/%/vector: export PAIR =$(subst /, ,$(@:target/%/vector=%)) ## Build with Cross
target/%/vector: export TRIPLE ?=$(word 1,${PAIR})
target/%/vector: export PROFILE ?=$(word 2,${PAIR})
target/%/vector: export RUSTFLAGS += -C link-arg=-s
target/%/vector: cargo-install-cross CARGO_HANDLES_FRESHNESS
	$(MAKE) -k cross-image-${TRIPLE}
	cross build \
		$(if $(findstring release,$(PROFILE)),--release,) \
		--target ${TRIPLE} \
		--no-default-features \
		--features target-${TRIPLE}

target/%/vector.tar.gz: export PAIR =$(subst /, ,$(@:target/%/vector.tar.gz=%)) ## Create archive
target/%/vector.tar.gz: export TRIPLE ?=$(word 1,${PAIR})
target/%/vector.tar.gz: export PROFILE ?=$(word 2,${PAIR})
target/%/vector.tar.gz: target/%/vector CARGO_HANDLES_FRESHNESS
	rm -rf target/scratch/vector-${TRIPLE} || true
	mkdir -p target/scratch/vector-${TRIPLE}/bin target/scratch/vector-${TRIPLE}/etc
	cp --recursive --force --verbose \
		target/${TRIPLE}/${PROFILE}/vector \
		target/scratch/vector-${TRIPLE}/bin/vector
	cp --recursive --force --verbose \
		README.md \
		LICENSE \
		config \
		target/scratch/vector-${TRIPLE}/
	cp --recursive --force --verbose \
		distribution/systemd \
		target/scratch/vector-${TRIPLE}/etc/
	tar --create \
		--gzip \
		--verbose \
		--file target/${TRIPLE}/${PROFILE}/vector.tar.gz \
		--directory target/scratch/ \
		./vector-${TRIPLE}
	rm -rf target/scratch/

.PHONY: cross-image-%
cross-image-%: export TRIPLE =$($(strip @):cross-image-%=%) ## Build Cross images
cross-image-%:
	$(CONTAINER_TOOL) build \
		--tag vector-cross-env:${TRIPLE} \
		--file scripts/cross/${TRIPLE}.dockerfile \
		scripts/cross

##@ Packaging

target/artifacts/vector-%.tar.gz: export TRIPLE :=$(@:target/artifacts/vector-%.tar.gz=%)
target/artifacts/vector-%.tar.gz: override PROFILE =release
target/artifacts/vector-%.tar.gz: target/%/release/vector.tar.gz
	@echo "Built to ${<}, relocating to ${@}"
	@mkdir -p target/artifacts/
	@cp -v \
		${<} \
		${@}

.PHONY: package
package: build ## Build the Vector archive
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/package-archive.sh

.PHONY: package-x86_64-unknown-linux-gnu-all
package-x86_64-unknown-linux-gnu-all: package-x86_64-unknown-linux-gnu package-deb-x86_64-unknown-linux-gnu package-rpm-x86_64-unknown-linux-gnu # Build all x86_64 GNU packages

.PHONY: package-x86_64-unknown-linux-musl-all
package-x86_64-unknown-linux-musl-all: package-x86_64-unknown-linux-musl package-rpm-x86_64-unknown-linux-musl # Build all x86_64 MUSL packages

.PHONY: package-aarch64-unknown-linux-musl-all
package-aarch64-unknown-linux-musl-all: package-aarch64-unknown-linux-musl package-deb-aarch64 package-rpm-aarch64  # Build all aarch64 MUSL packages

.PHONY: package-armv7-unknown-linux-gnueabihf-all
package-armv7-unknown-linux-gnueabihf-all: package-armv7-unknown-linux-gnueabihf package-deb-armv7-gnu package-rpm-armv7-gnu  # Build all armv7-unknown-linux-gnueabihf MUSL packages

.PHONY: package-x86_64-unknown-linux-gnu
package-x86_64-unknown-linux-gnu: target/artifacts/vector-x86_64-unknown-linux-gnu.tar.gz ## Build an archive suitable for the `x86_64-unknown-linux-gnu` triple.
	@echo "Output to ${<}."

.PHONY: package-x86_64-unknown-linux-musl
package-x86_64-unknown-linux-musl: target/artifacts/vector-x86_64-unknown-linux-musl.tar.gz ## Build an archive suitable for the `x86_64-unknown-linux-musl` triple.
	@echo "Output to ${<}."

.PHONY: package-aarch64-unknown-linux-musl
package-aarch64-unknown-linux-musl: target/artifacts/vector-aarch64-unknown-linux-musl.tar.gz ## Build an archive suitable for the `aarch64-unknown-linux-musl` triple.
	@echo "Output to ${<}."

.PHONY: package-aarch64-unknown-linux-gnu
package-aarch64-unknown-linux-gnu: target/artifacts/vector-aarch64-unknown-linux-gnu.tar.gz ## Build an archive suitable for the `aarch64-unknown-linux-gnu` triple.
	@echo "Output to ${<}."

.PHONY: package-armv7-unknown-linux-gnueabihf
package-armv7-unknown-linux-gnueabihf: target/artifacts/vector-armv7-unknown-linux-gnueabihf.tar.gz ## Build an archive suitable for the `armv7-unknown-linux-gnueabihf` triple.
	@echo "Output to ${<}."

.PHONY: package-armv7-unknown-linux-musleabihf
package-armv7-unknown-linux-musleabihf: target/artifacts/vector-armv7-unknown-linux-musleabihf.tar.gz ## Build an archive suitable for the `armv7-unknown-linux-musleabihf triple.
	@echo "Output to ${<}."

# debs

.PHONY: package-deb-x86_64-unknown-linux-gnu
package-deb-x86_64-unknown-linux-gnu: package-x86_64-unknown-linux-gnu ## Build the x86_64 GNU deb package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=x86_64-unknown-linux-gnu timberio/ci_image ./scripts/package-deb.sh

.PHONY: package-deb-x86_64-unknown-linux-musl
package-deb-x86_64-unknown-linux-musl: package-x86_64-unknown-linux-musl ## Build the x86_64 GNU deb package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=x86_64-unknown-linux-musl timberio/ci_image ./scripts/package-deb.sh

.PHONY: package-deb-aarch64
package-deb-aarch64: package-aarch64-unknown-linux-musl  ## Build the aarch64 deb package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=aarch64-unknown-linux-musl timberio/ci_image ./scripts/package-deb.sh

.PHONY: package-deb-armv7-gnu
package-deb-armv7-gnu: package-armv7-unknown-linux-gnueabihf ## Build the armv7-unknown-linux-gnueabihf deb package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=armv7-unknown-linux-gnueabihf timberio/ci_image ./scripts/package-deb.sh

# rpms

.PHONY: package-rpm-x86_64-unknown-linux-gnu
package-rpm-x86_64-unknown-linux-gnu: package-x86_64-unknown-linux-gnu ## Build the x86_64 rpm package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=x86_64-unknown-linux-gnu timberio/ci_image ./scripts/package-rpm.sh

.PHONY: package-rpm-x86_64-unknown-linux-musl
package-rpm-x86_64-unknown-linux-musl: package-x86_64-unknown-linux-musl ## Build the x86_64 musl rpm package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=x86_64-unknown-linux-musl timberio/ci_image ./scripts/package-rpm.sh

.PHONY: package-rpm-aarch64
package-rpm-aarch64: package-aarch64-unknown-linux-musl ## Build the aarch64 rpm package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=aarch64-unknown-linux-musl timberio/ci_image ./scripts/package-rpm.sh

.PHONY: package-rpm-armv7-gnu
package-rpm-armv7-gnu: package-armv7-unknown-linux-gnueabihf ## Build the armv7-unknown-linux-gnueabihf rpm package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=armv7-unknown-linux-gnueabihf timberio/ci_image ./scripts/package-rpm.sh
