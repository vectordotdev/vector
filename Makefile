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
			--mount type=bind,source=${PWD},target=/git/timberio/vector \
			--mount type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock \
			--mount type=volume,source=vector-target,target=/git/timberio/vector/target \
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

build-all: build-x86_64-unknown-linux-musl build-aarch64-unknown-linux-musl ## Build the project in release mode for all supported platforms

build-x86_64-unknown-linux-gnu: ## Build dynamically linked binary in release mode for the x86_64 architecture
	$(RUN) build-x86_64-unknown-linux-gnu

build-x86_64-unknown-linux-musl: ## Build static binary in release mode for the x86_64 architecture
	$(RUN) build-x86_64-unknown-linux-musl

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

stop-test-integration: ## Stops all integration test infrastructure
stop-test-integration: stop-integration-aws stop-integration-clickhouse stop-integration-elasticsearch
stop-test-integration: stop-integration-gcp stop-integration-influxdb stop-integration-kafka stop-integration-loki
stop-test-integration: stop-integration-pulsar stop-integration-splunk

start-integration-aws:
	$(CONTAINER_TOOL) network create test-integration-aws
	$(CONTAINER_TOOL) run -d --network=test-integration-aws -p 8111:8111 --name ec2_metadata \
	 timberiodev/mock-ec2-metadata:latest
	$(CONTAINER_TOOL) run -d --network=test-integration-aws -p 4568:4568 -p 4572:4572 -p 4582:4582 -p 4571:4571 -p 4573:4573 \
	 --name localstack -e SERVICES=kinesis:4568,s3:4572,cloudwatch:4582,elasticsearch:4571,firehose:4573 \
	 localstack/localstack@sha256:f21f1fc770ee4bfd5012afdc902154c56b7fb18c14cf672de151b65569c8251e
	$(CONTAINER_TOOL) run -d --network=test-integration-aws -p 6000:6000 --name mockwatchlogs \
	 -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
	sleep 5

stop-integration-aws:
	$(CONTAINER_TOOL) rm --force ec2_metadata mockwatchlogs localstack 2>/dev/null; true
	$(CONTAINER_TOOL) network rm test-integration-aws 2>/dev/null; true

test-integration-aws: AWS_ACCESS_KEY_ID ?= "dummy"
test-integration-aws: AWS_SECRET_ACCESS_KEY ?= "dummy"
test-integration-aws: ## Runs AWS integration tests
ifeq ($(AUTOSPAWN), true)
	$(MAKE) -k stop-integration-aws \
    ; rc=$$? \
	$(MAKE) start-integration-aws
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features aws-integration-tests --lib ::aws_ -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-aws
endif

start-integration-clickhouse:
	$(CONTAINER_TOOL) network create test-integration-clickhouse
	$(CONTAINER_TOOL) run -d --network=test-integration-clickhouse -p 8123:8123 --name clickhouse yandex/clickhouse-server:19

stop-integration-clickhouse:
	$(CONTAINER_TOOL) rm --force clickhouse 2>/dev/null; true
	$(CONTAINER_TOOL) network rm test-integration-clickhouse 2>/dev/null; true

test-integration-clickhouse: ## Runs Clickhouse integration tests
ifeq ($(AUTOSPAWN), true)
	$(MAKE) -k stop-integration-clickhouse \
    ; rc=$$? \
	$(MAKE) start-integration-clickhouse
	sleep 5 # Many services are very slow... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features clickhouse-integration-tests --lib ::clickhouse:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-clickhouse
endif

test-integration-docker: ## Runs Docker integration tests
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features docker-integration-tests --lib ::docker:: -- --nocapture

start-integration-elasticsearch:
	$(CONTAINER_TOOL) network create test-integration-elasticsearch
	$(CONTAINER_TOOL) run -d --network=test-integration-elasticsearch -p 4571:4571 --name localstack \
	 -e SERVICES=elasticsearch:4571 localstack/localstack@sha256:f21f1fc770ee4bfd5012afdc902154c56b7fb18c14cf672de151b65569c8251e
	$(CONTAINER_TOOL) run -d --network=test-integration-elasticsearch -p 9200:9200 -p 9300:9300 \
	 --name elasticsearch -e discovery.type=single-node -e ES_JAVA_OPTS="-Xms400m -Xmx400m" elasticsearch:6.6.2
	$(CONTAINER_TOOL) run -d --network=test-integration-elasticsearch -p 9201:9200 -p 9301:9300 \
	 --name elasticsearch-tls -e discovery.type=single-node -e xpack.security.enabled=true \
	 -e xpack.security.http.ssl.enabled=true -e xpack.security.transport.ssl.enabled=true \
	 -e xpack.ssl.certificate=certs/localhost.crt -e xpack.ssl.key=certs/localhost.key \
	 -e ES_JAVA_OPTS="-Xms400m -Xmx400m" \
	 -v $(PWD)/tests/data:/usr/share/elasticsearch/config/certs:ro elasticsearch:6.6.2

stop-integration-elasticsearch:
	$(CONTAINER_TOOL) rm --force localstack elasticsearch elasticsearch-tls 2>/dev/null; true
	$(CONTAINER_TOOL) network rm test-integration-elasticsearch 2>/dev/null; true

test-integration-elasticsearch: AWS_ACCESS_KEY_ID ?= "dummy"
test-integration-elasticsearch: AWS_SECRET_ACCESS_KEY ?= "dummy"
test-integration-elasticsearch: ## Runs Elasticsearch integration tests
ifeq ($(AUTOSPAWN), true)
	$(MAKE) -k stop-integration-elasticsearch \
    ; rc=$$? \
	$(MAKE) start-integration-elasticsearch
	sleep 60 # Many services are very slow... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features es-integration-tests --lib ::elasticsearch:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-elasticsearch
endif

start-integration-gcp:
	$(CONTAINER_TOOL) network create test-integration-gcp
	$(CONTAINER_TOOL) run -d --network=test-integration-gcp -p 8681-8682:8681-8682 --name cloud-pubsub \
	 -e PUBSUB_PROJECT1=testproject,topic1:subscription1 messagebird/gcloud-pubsub-emulator

stop-integration-gcp:
	$(CONTAINER_TOOL) rm --force cloud-pubsub 2>/dev/null; true
	$(CONTAINER_TOOL) network rm test-integration-gcp 2>/dev/null; true

test-integration-gcp: ## Runs GCP integration tests
ifeq ($(AUTOSPAWN), true)
	$(MAKE) -k stop-integration-gcp \
    ; rc=$$? \
	$(MAKE) start-integration-gcp
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features gcp-integration-tests --lib ::gcp:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-gcp
endif

start-integration-humio:
	$(CONTAINER_TOOL) network create test-integration-humio
	$(CONTAINER_TOOL) run -d --network=test-integration-humio -p 8080:8080 --name humio humio/humio:1.13.1

stop-integration-humio:
	$(CONTAINER_TOOL) rm --force humio 2>/dev/null; true
	$(CONTAINER_TOOL) network rm test-integration-humio 2>/dev/null; true

test-integration-humio: ## Runs Humio integration tests
ifeq ($(AUTOSPAWN), true)
	$(MAKE) -k stop-integration-humio \
    ; rc=$$? \
	$(MAKE) start-integration-humio
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features humio-integration-tests --lib ::humio:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-humio
endif

start-integration-influxdb:
	$(CONTAINER_TOOL) network create test-integration-influxdb
	$(CONTAINER_TOOL) run -d --network=test-integration-influxdb -p 8086:8086 --name influxdb_v1 \
	 -e INFLUXDB_REPORTING_DISABLED=true influxdb:1.7
	$(CONTAINER_TOOL) run -d --network=test-integration-influxdb -p 9999:9999 --name influxdb_v2 \
	 -e INFLUXDB_REPORTING_DISABLED=true  quay.io/influxdb/influxdb:2.0.0-beta influxd --reporting-disabled

stop-integration-influxdb:
	$(CONTAINER_TOOL) rm --force influxdb_v1 influxdb_v2 2>/dev/null; true
	$(CONTAINER_TOOL) network rm test-integration-influxdb 2>/dev/null; true

test-integration-influxdb: ## Runs InfluxDB integration tests
ifeq ($(AUTOSPAWN), true)
	$(MAKE) -k stop-integration-influxdb \
    ; rc=$$? \
	$(MAKE) start-integration-influxdb
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features influxdb-integration-tests --lib ::influxdb::integration_tests:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-influxdb
endif

start-integration-kafka:
	$(CONTAINER_TOOL) network create test-integration-kafka
	$(CONTAINER_TOOL) run -d --network=test-integration-kafka -p 2181:2181 --name zookeeper wurstmeister/zookeeper
	$(CONTAINER_TOOL) run -d --network=test-integration-kafka -p 9091-9093:9091-9093 -e KAFKA_BROKER_ID=1 \
	 -e KAFKA_ZOOKEEPER_CONNECT=zookeeper:2181 -e KAFKA_LISTENERS=PLAINTEXT://:9091,SSL://:9092,SASL_PLAINTEXT://:9093 \
	 -e KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://localhost:9091,SSL://localhost:9092,SASL_PLAINTEXT://localhost:9093 \
	 -e KAFKA_SSL_KEYSTORE_LOCATION=/certs/localhost.p12 -e KAFKA_SSL_KEYSTORE_PASSWORD=NOPASS \
	 -e KAFKA_SSL_TRUSTSTORE_LOCATION=/certs/localhost.p12 -e KAFKA_SSL_TRUSTSTORE_PASSWORD=NOPASS \
	 -e KAFKA_SSL_KEY_PASSWORD=NOPASS -e KAFKA_SSL_ENDPOINT_IDENTIFICATION_ALGORITHM=none \
	 -e KAFKA_OPTS="-Djava.security.auth.login.config=/etc/kafka/kafka_server_jaas.conf" \
	 -e KAFKA_INTER_BROKER_LISTENER_NAME=SASL_PLAINTEXT -e KAFKA_SASL_ENABLED_MECHANISMS=PLAIN \
	 -e KAFKA_SASL_MECHANISM_INTER_BROKER_PROTOCOL=PLAIN -v $(PWD)/tests/data/localhost.p12:/certs/localhost.p12:ro \
	 -v $(PWD)/tests/data/kafka_server_jaas.conf:/etc/kafka/kafka_server_jaas.conf --name kafka wurstmeister/kafka

stop-integration-kafka:
	$(CONTAINER_TOOL) rm --force kafka zookeeper 2>/dev/null; true
	$(CONTAINER_TOOL) network rm test-integration-kafka 2>/dev/null; true

test-integration-kafka: ## Runs Kafka integration tests
ifeq ($(AUTOSPAWN), true)
	$(MAKE) -k stop-integration-kafka \
    ; rc=$$? \
	$(MAKE) start-integration-kafka
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features "kafka-integration-tests rdkafka-plain" --lib ::kafka:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-kafka
endif

start-integration-loki:
	$(CONTAINER_TOOL) network create test-integration-loki
	$(CONTAINER_TOOL) run -d --network=test-integration-loki -p 3100:3100 -v $(PWD)/tests/data:/etc/loki \
	 --name loki grafana/loki:master -config.file=/etc/loki/loki-config.yaml

stop-integration-loki:
	$(CONTAINER_TOOL) rm --force loki 2>/dev/null; true
	$(CONTAINER_TOOL) network rm test-integration-loki 2>/dev/null; true

test-integration-loki: ## Runs Loki integration tests
ifeq ($(AUTOSPAWN), true)
	$(MAKE) -k stop-integration-loki \
    ; rc=$$? \
	$(MAKE) start-integration-loki
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features loki-integration-tests --lib ::loki:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-loki
endif

start-integration-pulsar:
	$(CONTAINER_TOOL) network create test-integration-pulsar
	$(CONTAINER_TOOL) run -d --network=test-integration-pulsar -p 6650:6650 --name pulsar \
	 apachepulsar/pulsar bin/pulsar standalone

stop-integration-pulsar:
	$(CONTAINER_TOOL) rm --force pulsar 2>/dev/null; true
	$(CONTAINER_TOOL) network rm test-integration-pulsar 2>/dev/null; true

test-integration-pulsar: ## Runs Pulsar integration tests
ifeq ($(AUTOSPAWN), true)
	$(MAKE) -k stop-integration-pulsar \
    ; rc=$$? \
	$(MAKE) start-integration-pulsar
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features pulsar-integration-tests --lib ::pulsar:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-pulsar
endif

start-integration-splunk:
	$(CONTAINER_TOOL) network create test-integration-splunk
	$(CONTAINER_TOOL) run -d --network=test-integration-splunk -p 8088:8088 -p 8000:8000 -p 8089:8089 \
     --name splunk timberio/splunk-hec-test:minus_compose

stop-integration-splunk:
	$(CONTAINER_TOOL) rm --force splunk 2>/dev/null; true
	$(CONTAINER_TOOL) network rm test-integration-splunk 2>/dev/null; true

test-integration-splunk: ## Runs Splunk integration tests
ifeq ($(AUTOSPAWN), true)
	$(MAKE) -k stop-integration-splunk \
    ; rc=$$? \
	$(MAKE) start-integration-splunk
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features splunk-integration-tests --lib ::splunk_hec:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-splunk
endif

PACKAGE_DEB_USE_CONTAINER ?= $(USE_CONTAINER)
test-e2e-kubernetes: ## Runs Kubernetes E2E tests (Sorry, no `ENVIRONMENT=true` support)
	PACKAGE_DEB_USE_CONTAINER="$(PACKAGE_DEB_USE_CONTAINER)" scripts/test-e2e-kubernetes.sh

test-shutdown: ## Runs shutdown tests
ifeq ($(AUTOSPAWN), true)
	$(MAKE) -k stop-integration-kafka \
    ; rc=$$? \
	$(MAKE) start-integration-kafka
	sleep 30 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features shutdown-tests --test shutdown -- --test-threads 4
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-kafka
endif

test-cli: ## Runs cli tests
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --test cli -- --test-threads 4

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
	${MAYBE_ENVIRONMENT_EXEC} cargo test wasm --no-default-features --features "wasm" --lib -- --nocapture

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

check-all: check-fmt check-clippy check-style check-markdown check-meta check-version check-examples check-component-features check-scripts ## Check everything

check-component-features: ## Check that all component features are setup properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-component-features.sh

check-clippy: ## Check code with Clippy
	${MAYBE_ENVIRONMENT_EXEC} cargo clippy --workspace --all-targets -- -D warnings

check-fmt: ## Check that all files are formatted properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-fmt.sh

check-style: ## Check that all files are styled properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-style.sh

check-markdown: ## Check that markdown is styled properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-markdown.sh

check-meta: ## Check that all /.meta file are valid
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-meta.sh

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

package-aarch64-unknown-linux-musl-all: package-archive-aarch64-unknown-linux-musl package-deb-aarch64 package-rpm-aarch64  # Build all aarch64 MUSL packages

# archives

package-archive: build ## Build the Vector archive
	$(RUN) package-archive

package-archive-all: package-archive-x86_64-unknown-linux-musl package-archive-x86_64-unknown-linux-gnu package-archive-aarch64-unknown-linux-musl ## Build all archives

package-archive-x86_64-unknown-linux-musl: build-x86_64-unknown-linux-musl ## Build the x86_64 archive
	$(RUN) package-archive-x86_64-unknown-linux-musl

package-archive-x86_64-unknown-linux-gnu: build-x86_64-unknown-linux-gnu ## Build the x86_64 archive
	$(RUN) package-archive-x86_64-unknown-linux-gnu

package-archive-aarch64-unknown-linux-musl: build-aarch64-unknown-linux-musl ## Build the aarch64 archive
	$(RUN) package-archive-aarch64-unknown-linux-musl

# debs

package-deb: ## Build the deb package
	$(RUN) package-deb

package-deb-all: package-deb-x86_64 ## Build all deb packages

package-deb-x86_64: package-archive-x86_64-unknown-linux-gnu ## Build the x86_64 deb package
	$(RUN) package-deb-x86_64

package-deb-aarch64: package-archive-aarch64-unknown-linux-musl  ## Build the aarch64 deb package
	$(RUN) package-deb-aarch64

# rpms

package-rpm: ## Build the rpm package
	@scripts/package-rpm.sh

package-rpm-all: package-rpm-x86_64 package-rpm-aarch64 ## Build all rpm packages

package-rpm-x86_64: package-archive-x86_64-unknown-linux-gnu ## Build the x86_64 rpm package
	$(RUN) package-rpm-x86_64

package-rpm-aarch64: package-archive-aarch64-unknown-linux-musl ## Build the aarch64 rpm package
	$(RUN) package-rpm-aarch64

##@ Releasing

release: release-prepare generate release-commit ## Release a new Vector version

release-commit: ## Commits release changes
	@scripts/release-commit.rb

release-docker: ## Release to Docker Hub
	@scripts/release-docker.sh

release-github: ## Release to Github
	@scripts/release-github.rb

release-homebrew: ## Release to timberio Homebrew tap
	@scripts/release-homebrew.sh

release-prepare: ## Prepares the release with metadata and highlights
	@scripts/release-prepare.rb

release-push: ## Push new Vector version
	@scripts/release-push.sh

release-rollback: ## Rollback pending release changes
	@scripts/release-rollback.rb

release-s3: ## Release artifacts to S3
	@scripts/release-s3.sh

release-helm: ## Package and release Helm Chart
	@scripts/release-helm.sh

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

verify-deb: ## Verify all deb packages
verify-deb: verify-deb-artifact-on-deb-8 verify-deb-artifact-on-deb-9 verify-deb-artifact-on-deb-10
verify-deb: verify-deb-artifact-on-ubuntu-14-04 verify-deb-artifact-on-ubuntu-16-04 verify-deb-artifact-on-ubuntu-18-04 verify-deb-artifact-on-ubuntu-20-04

verify-deb-artifact-on-deb-8: package-deb-x86_64 ## Verify the deb package on Debian 8
	$(RUN) verify-deb-artifact-on-deb-8

verify-deb-artifact-on-deb-9: package-deb-x86_64 ## Verify the deb package on Debian 9
	$(RUN) verify-deb-artifact-on-deb-9

verify-deb-artifact-on-deb-10: package-deb-x86_64 ## Verify the deb package on Debian 10
	$(RUN) verify-deb-artifact-on-deb-10

verify-deb-artifact-on-ubuntu-14-04: package-deb-x86_64 ## Verify the deb package on Ubuntu 14.04
	$(RUN) verify-deb-artifact-on-ubuntu-14-04

verify-deb-artifact-on-ubuntu-16-04: package-deb-x86_64 ## Verify the deb package on Ubuntu 16.04
	$(RUN) verify-deb-artifact-on-ubuntu-16-04

verify-deb-artifact-on-ubuntu-18-04: package-deb-x86_64 ## Verify the deb package on Ubuntu 18.04
	$(RUN) verify-deb-artifact-on-ubuntu-18-04

verify-deb-artifact-on-ubuntu-20-04: package-deb-x86_64 ## Verify the deb package on Ubuntu 20.04
	$(RUN) verify-deb-artifact-on-ubuntu-20-04

verify-nixos:  ## Verify that Vector can be built on NixOS
	$(RUN) verify-nixos

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
	scripts/signoff.sh

slim-builds: ## Updates the Cargo config to product disk optimized builds (for CI, not for users)
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/slim-builds.sh

target-graph: ## Display dependencies between targets in this Makefile
	@cd $(shell realpath $(shell dirname $(firstword $(MAKEFILE_LIST)))) && docker-compose run --rm target-graph $(TARGET)

version: ## Get the current Vector version
	@scripts/version.sh

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
