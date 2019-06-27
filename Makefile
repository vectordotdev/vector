.PHONY: help
.DEFAULT_GOAL := help

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
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

bench: ## Run internal benchmarks
	@cargo bench

build: ## Build Vector for a given $TARGET and $VERSION
	@scripts/build.sh

generate-docs: ## Generate docs from the scipts/config_schema.toml file
	@bundle install --gemfile=scripts/sync_config_schema/Gemfile
	@scripts/sync_config_schema.sh

package-deb: ## Create a .deb package from artifacts created via `build`
	@scripts/package-deb.sh

package-rpm: ## Create a .rpm package from artifacts created via `build`
	@scripts/package-rpm.sh

release-package-cloud: ## Release packages to packagecloud.io
	@scripts/release_package_cloud.sh

test: ## Run tests
	@docker-compose up -d
	@cargo test --all --features docker -- --test-threads 4

version: ## Get the current Vector version
	@scripts/version.sh