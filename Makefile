.PHONY: help
.DEFAULT_GOAL := help
_latest_version := $(shell scripts/version.sh true)
_version := $(shell scripts/version.sh)
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
	@awk 'BEGIN {FS = ":.*##"; printf "Usage: make \033[36m<target>\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-25s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Development

bench: ## Run internal benchmarks
	@cargo bench --all

build: ## Build the project in release mode
	@cargo build --no-default-features --features="$${FEATURES:-default}" --release

check: check-code check-fmt check-generate check-examples

check-blog: ## Checks that all blog articles are signed by their authors
	@scripts/run.sh checker scripts/check-blog-signatures.rb

check-code: ## Checks code for compilation errors (only default features)
	@scripts/run.sh checker cargo check --all --all-targets --features docker,kubernetes

check-component-features: ## Checks that all component are behind corresponding features
	@scripts/run.sh checker-component-features scripts/check-component-features.sh

check-examples: ## Validates the config examples
	@cargo run -q -- validate --topology --deny-warnings ./config/examples/*.toml

check-fmt: ## Checks code formatting correctness
	@scripts/run.sh checker scripts/check-style.sh
	@scripts/run.sh checker cargo fmt -- --check

check-generate: ## Checks for pending `make generate` changes
	@scripts/run.sh checker scripts/check-generate.sh

check-markdown: ## Check Markdown style
	@scripts/run.sh checker-markdown markdownlint .

check-version: ## Checks that the version in Cargo.toml is up-to-date
	@scripts/run.sh checker scripts/check-version.rb

clean: ## Remove build artifacts
	@cargo clean

fmt: ## Format code
	@scripts/check-style.sh --fix
	@cargo fmt

export CHECK_URLS ?= true
generate: ## Generates files across the repo using the data in /.meta
	@scripts/run.sh checker scripts/generate.rb

release: ## Release a new Vector version
	@$(MAKE) release-prepare
	@$(MAKE) generate CHECK_URLS=false
	@$(MAKE) release-commit

release-push: ## Push new Vector version
	@scripts/release-push.sh

run: ## Starts Vector in development mode
	@cargo run --no-default-features --features ${DEFAULT_FEATURES}

signoff: ## Signsoff all previous commits since branch creation
	@scripts/signoff.sh

export ARTICLE ?= true
sign-blog: ## Sign newly added blog articles using GPG
	@scripts/sign-blog.sh

slim-builds: ## Updates the Cargo config to product disk optimized builds, useful for CI
	@scripts/slim-builds.sh

test: test-behavior test-integration test-unit

test-behavior: ## Runs behavioral tests
	@cargo run --no-default-features --features ${DEFAULT_FEATURES} -- test tests/behavior/**/*.toml

test-integration:
	@cargo test --no-default-features --features ${DEFAULT_FEATURES} --all --features docker --no-run
	@docker-compose up -d test-runtime-deps
	@cargo test --no-default-features --features ${DEFAULT_FEATURES} --all --features docker -- --test-threads 4

test-integration-aws: ## Runs Clickhouse integration tests
	@docker-compose up -d localstack mockwatchlogs ec2_metadata minio
	@cargo test --no-default-features --features cloudwatch-logs-integration-tests,cloudwatch-metrics-integration-tests,ec2-metadata-integration-tests,firehose-integration-tests,kinesis-integration-tests,s3-integration-tests

test-integration-clickhouse: ## Runs Clickhouse integration tests
	@docker-compose up -d clickhouse
	@cargo test --no-default-features --features clickhouse-integration-tests

test-integration-docker: ## Runs Docker integration tests
	@cargo test --no-default-features --features docker-integration-tests

test-integration-elasticsearch: ## Runs Elasticsearch integration tests
	@docker-compose up -d elasticsearch elasticsearch-tls localstack
	@cargo test --no-default-features --features es-integration-tests

test-integration-gcp: ## Runs GCP integration tests
	@docker-compose up -d gcloud-pubsub
	@cargo test --no-default-features --features gcp-pubsub-integration-tests, gcs-integration-tests

test-integration-influxdb: ## Runs Kafka integration tests
	@docker-compose up -d influxdb_v1 influxdb_v2
	@cargo test --no-default-features --features influxdb-integration-tests

test-integration-kafka: ## Runs Kafka integration tests
	@docker-compose up -d kafka
	@cargo test --no-default-features --features kafka-integration-tests

test-integration-kubernetes: ## Runs Kubernetes integration tests
	@docker-compose up -d kafka
	@cargo test --no-default-features --features kafka-integration-tests

test-integration-pulsar: ## Runs Kafka integration tests
	@docker-compose up -d pulsar
	@cargo test --no-default-features --features pulsar-integration-tests

test-integration-splunk: ## Runs Kafka integration tests
	@docker-compose up -d splunk
	@cargo test --no-default-features --features splunk-integration-tests

test-unit: ## Runs unit tests that do not require network dependencies
	@cargo test --no-run --target ${TARGET}

##@ Releasing

build-archive: ## Build a Vector archive for a given $TARGET and $VERSION
	scripts/build-archive.sh

build-ci-docker-images: ## Build the various Docker images used for CI
	@scripts/build-ci-docker-images.sh

build-docker: ## Build the Vector docker images from artifacts created via `package-deb`, but do not push
	@scripts/build-docker.sh

package-deb: ## Create a .deb package from artifacts created via `build-archive`
	@scripts/package-deb.sh

package-rpm: ## Create a .rpm package from artifacts created via `build-archive`
	@scripts/package-rpm.sh

release-commit: ## Commits release changes
	scripts/release-commit.rb

release-docker: ## Release to Docker Hub
	@scripts/release-docker.sh

release-github: ## Release to Github
	@bundle install --gemfile=scripts/Gemfile --quiet
	@scripts/release-github.rb

release-homebrew: ## Release to timberio Homebrew tap
	@scripts/release-homebrew.sh

release-prepare: ## Prepares the release with metadata and highlights
	@scripts/run.sh checker scripts/release-prepare.rb

release-rollback:
	@scripts/run.sh checker scripts/release-rollback.rb

release-s3: ## Release artifacts to S3
	@scripts/release-s3.sh

sync-install:
	@aws s3 cp distribution/install.sh s3://sh.vector.dev --sse --acl public-read

version: ## Get the current Vector version
	@echo $(_version)
