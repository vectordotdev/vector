##@ Utility

.PHONY: build-ci-docker-images
build-ci-docker-images: ## Rebuilds all Docker images used in CI
	@scripts/build-ci-docker-images.sh

.PHONY: clean
clean: environment-clean ## Clean everything
	cargo clean

.PHONY: fmt
fmt: ## Format code
	${MAYBE_ENVIRONMENT_EXEC} cargo fmt
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-style.sh --fix

.PHONY: signoff
signoff: ## Signsoff all previous commits since branch creation
	scripts/signoff.sh

.PHONY: slim-builds
slim-builds: ## Updates the Cargo config to product disk optimized builds (for CI, not for users)
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/slim-builds.sh

ifeq (${CI}, true)
.PHONY: ci-sweep
ci-sweep: ## Sweep up the CI to try to get more disk space.
	@echo "Preparing the CI for build by sweeping up disk space a bit..."
	df -h
	sudo apt-get --purge autoremove
	sudo apt-get clean
	sudo rm -rf "/opt/*" "/usr/local/*"
	sudo rm -rf "/usr/local/share/boost" && sudo rm -rf "${AGENT_TOOLSDIRECTORY}"
	docker system prune --force
	df -h
endif

.PHONY: version
version: ## Get the current Vector version
	@scripts/version.sh

.PHONY: git-hooks
git-hooks: ## Add Vector-local git hooks for commit sign-off
	@scripts/install-git-hooks.sh

.PHONY: update-helm-dependencies
update-helm-dependencies: ## Recursively update the dependencies of the Helm charts in the proper order
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/helm-dependencies.sh update

.PHONY: update-kubernetes-yaml
update-kubernetes-yaml: ## Regenerate the Kubernetes YAML config
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/kubernetes-yaml.sh update

.PHONY: cargo-install-%
cargo-install-%: override TOOL = $(@:cargo-install-%=%)
cargo-install-%:
	$(if $(findstring true,$(AUTOINSTALL)),cargo install ${TOOL} --quiet,)

.PHONY: ensure-has-wasm-toolchain ### Configures a wasm toolchain for test artifact building, if required
ensure-has-wasm-toolchain: target/wasm32-wasi/.obtained
target/wasm32-wasi/.obtained:
	@echo "# You should also install WABT for WASM module development!"
	@echo "# You can use your package manager or check https://github.com/WebAssembly/wabt"
	${MAYBE_ENVIRONMENT_EXEC} rustup target add wasm32-wasi
	@mkdir -p target/wasm32-wasi
	@touch target/wasm32-wasi/.obtained
