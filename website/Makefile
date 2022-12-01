CUE = ./scripts/cue.sh

clean:
	rm -rf public resources data/docs.json

setup:
	yarn

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

serve: clean setup structured-data
	hugo server \
	--buildDrafts \
	--buildFuture \
	--environment "development"

# Live site
production-build:
	hugo \
	--environment "production" \
	--minify

ci-production-build: setup structured-data production-build run-link-checker algolia

# Preview site
preview-build:
	hugo \
	--baseURL $(DEPLOY_PRIME_URL) \
	--buildFuture \
	--environment "preview" \
	--minify
	cp ./custom-headers/_preview ./public/_headers

ci-preview-build: setup structured-data preview-build run-link-checker algolia

run-link-checker:
	htmltest

run-external-link-checker:
	htmltest --conf ./.htmltest.external.yml

# Check links without rebuilding the CUE sources. Optimal for fixing links locally.
check-links: production-build run-link-checker

# Search
algolia:
	yarn algolia-config
	yarn algolia-index
	yarn algolia-upload

# Useful for locally debugging issues that arise only on the deployed production site
run-production-site-locally:
	make setup structured-data production-build
	python3 -m http.server 1313 --directory ./public --bind 127.0.0.1

# Local dev build with no link checking and no Yarn dependency fetching
quick-build: clean structured-data production-build

# Full local builds without Algolia updates (for debugging, link checking, etc.)
local-production-build: clean setup structured-data production-build run-link-checker

local-preview-build: clean setup structured-data preview-build run-link-checker

# Generate Lighthouse scores locally
lighthouse-report:
	yarn lighthouse
