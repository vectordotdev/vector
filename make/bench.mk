##@ Benching (Supports `ENVIRONMENT=true`)

.PHONY: bench
bench: ## Run benchmarks in /benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "benches"
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: bench-all
bench-all: ### Run default and WASM benches
bench-all: $(WASM_MODULE_OUTPUTS)
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "benches wasm-benches"

.PHONY: bench-wasm
bench-wasm: $(WASM_MODULE_OUTPUTS)  ### Run WASM benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "wasm-benches" --bench wasm wasm
