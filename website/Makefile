CUE = ./scripts/cue.sh

# Override to specify which network address to use in Hugo/HTTP servers for binding
export SERVER_BIND ?= 127.0.0.1
# Override to specify which port to use in Hugo/HTTP servers for listening
export SERVER_PORT ?= 1313

clean:
	rm -rf public resources data/cargo-lock.toml data/docs.json

setup:
	yarn

# Copy Cargo data such as the dependency information file
cargo-data:
	cp -f ../Cargo.lock data/cargo-lock.toml

# Build JSON from CUE sources
cue-build:
	${CUE} build

# Other CUE commands
cue-check:
	${CUE} check

cue-fmt:
	${CUE} fmt

cue-help:
	${CUE} help

cue-list:
	${CUE} list

cue-vet:
	${CUE} vet

config-examples:
	yarn config-examples

structured-data: cue-build config-examples

serve: clean setup cargo-data structured-data
	hugo server \
	--bind $(SERVER_BIND) \
	--port $(SERVER_PORT) \
	--buildDrafts \
	--buildFuture \
	--environment "development"

# Live site
production-build:
	hugo \
	--environment "production" \
	--minify

ci-production-build: setup cargo-data structured-data production-build run-link-checker typesense

# Preview site
preview-build:
	hugo \
	$(if $(DEPLOY_PRIME_URL),--baseURL $(DEPLOY_PRIME_URL),) \
	--buildFuture \
	--environment "preview" \
	--minify
	cp ./custom-headers/_preview ./public/_headers

ci-preview-build: setup cargo-data structured-data preview-build run-link-checker typesense

run-link-checker:
	htmltest

run-external-link-checker:
	htmltest --conf ./.htmltest.external.yml

# Check links without rebuilding the CUE sources. Optimal for fixing links locally.
check-links: production-build run-link-checker

# Search
typesense:
	yarn typesense-index
	yarn typesense-sync

# Useful for locally debugging issues that arise only on the deployed production site
run-production-site-locally:
	make setup cargo-data structured-data production-build
	python3 -m http.server $(SERVER_PORT) --bind $(SERVER_BIND) --directory ./public

# Local dev build with no link checking and no Yarn dependency fetching
quick-build: clean cargo-data structured-data production-build

# Full local builds without Algolia updates (for debugging, link checking, etc.)
local-production-build: clean setup cargo-data structured-data production-build run-link-checker

local-preview-build: clean setup cargo-data structured-data preview-build run-link-checker

# Generate Lighthouse scores locally
lighthouse-report:
	yarn lighthouse
