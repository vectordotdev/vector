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

generate-docs: ## Generate docs from the scipts/metadata.toml file
	@bundle install --gemfile=scripts/config_schema/Gemfile
	@scripts/generate_docs.sh

package-deb: ## Create a .deb package from artifacts created via `build`
	@scripts/package-deb.sh

package-rpm: ## Create a .rpm package from artifacts created via `build`
	@scripts/package-rpm.sh

release: ## Release artifacts produces by build
	@scripts/release.sh

test: ## Run tests
	@docker-compose up -d
	@cargo test --all --features docker -- --test-threads 4

version: ## Get the current Vector version
	@scripts/version.sh