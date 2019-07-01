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

build-archive: ## Build a Vector archive for a given $TARGET and $VERSION
	@scripts/build-archive.sh

build-ci-docker-images: ## Build the various Docker images used for CI
	@scripts/build-ci-docker-images.sh

generate-docs: ## Generate docs from the scipts/metadata.toml file
	@bundle install --gemfile=scripts/config_schema/Gemfile
	@scripts/generate-docs.sh

package-deb: ## Create a .deb package from artifacts created via `build`
	@scripts/package-deb.sh

package-rpm: ## Create a .rpm package from artifacts created via `build`
	@scripts/package-rpm.sh

release: ## Interactive script that releases the next version (major or minor)
	@scripts/release.sh

release-deb: ## Release .deb via Package Cloud
	@scripts/release-deb.sh

release-docker: ## Release to Docker Hub
	@scripts/release-docker.sh

release-github: ## Release to Github
	@scripts/release-github.sh

release-homebrew: ## Release to timberio Homebrew tap
	@scripts/release-homebrew.sh

release-rpm: ## Release .rpm via Package Cloud
	@scripts/release-rpm.sh

release-s3: ## Release artifacts to S3
	@scripts/release-s3.sh

test: ## Run tests
	@cargo test --all --features docker -- --test-threads 4

version: ## Get the current Vector version
	@scripts/version.sh