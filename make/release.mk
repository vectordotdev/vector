##@ Releasing

.PHONY: release
release: release-prepare generate release-commit ## Release a new Vector version

.PHONY: release-commit
release-commit: ## Commits release changes
	@scripts/release-commit.rb

.PHONY: release-docker
release-docker: ## Release to Docker Hub
	@scripts/release-docker.sh

.PHONY: release-github
release-github: ## Release to Github
	@scripts/release-github.sh

.PHONY: release-homebrew
release-homebrew: ## Release to timberio Homebrew tap
	@scripts/release-homebrew.sh

.PHONY: release-prepare
release-prepare: ## Prepares the release with metadata and highlights
	@scripts/release-prepare.rb

.PHONY: release-push
release-push: ## Push new Vector version
	@scripts/release-push.sh

.PHONY: release-rollback
release-rollback: ## Rollback pending release changes
	@scripts/release-rollback.rb

.PHONY: release-s3
release-s3: ## Release artifacts to S3
	@scripts/release-s3.sh

.PHONY: release-helm
release-helm: ## Package and release Helm Chart
	@scripts/release-helm.sh

.PHONY: sync-install
sync-install: ## Sync the install.sh script for access via sh.vector.dev
	@aws s3 cp distribution/install.sh s3://sh.vector.dev --sse --acl public-read
