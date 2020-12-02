##@ Checking

.PHONY: check
check: ## Run prerequisite code checks
	${MAYBE_ENVIRONMENT_EXEC} cargo check --all --no-default-features --features ${DEFAULT_FEATURES}

.PHONY: check-all
check-all: ## Check everything
check-all: check-fmt check-clippy check-style check-markdown check-docs
check-all: check-version check-examples check-component-features
check-all: check-scripts
check-all: check-helm-lint check-helm-dependencies check-kubernetes-yaml

.PHONY: check-component-features
check-component-features: ## Check that all component features are setup properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-component-features.sh

.PHONY: check-clippy
check-clippy: ## Check code with Clippy
	${MAYBE_ENVIRONMENT_EXEC} cargo clippy --workspace --all-targets --features all-integration-tests -- -D warnings

.PHONY: check-docs
check-docs: ## Check that all /docs file are valid
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-docs.sh

.PHONY: check-fmt
check-fmt: ## Check that all files are formatted properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-fmt.sh

.PHONY: check-style
check-style: ## Check that all files are styled properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-style.sh

.PHONY: check-markdown
check-markdown: ## Check that markdown is styled properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-markdown.sh

.PHONY: check-version
check-version: ## Check that Vector's version is correct accounting for recent changes
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-version.rb

.PHONY: check-examples
check-examples: ## Check that the config/examples files are valid
	${MAYBE_ENVIRONMENT_EXEC} cargo run -- validate --topology --deny-warnings ./config/examples/*.toml

.PHONY: check-scripts
check-scripts: ## Check that scipts do not have common mistakes
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-scripts.sh

.PHONY: check-helm-lint
check-helm-lint: ## Check that Helm charts pass helm lint
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-helm-lint.sh

.PHONY: check-helm-dependencies
check-helm-dependencies: ## Check that Helm charts have up-to-date dependencies
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/helm-dependencies.sh validate

.PHONY: check-kubernetes-yaml
check-kubernetes-yaml: ## Check that the generated Kubernetes YAML config is up to date
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/kubernetes-yaml.sh check

check-events: ## Check that events satisfy patterns set in https://github.com/timberio/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-events.sh
