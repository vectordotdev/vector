clean:
	rm -rf public resources

setup:
	yarn

# Build JSON from CUE sources
build-cue:
	./scripts/build-cue.sh

serve: setup build-cue
	hugo server \
	--buildDrafts \
	--buildFuture

# Live site
production-build:
	hugo \
	--minify

ci-production-build: setup build-cue production-build run-link-checker algolia

# Preview site
preview-build:
	hugo \
	--baseURL $(DEPLOY_PRIME_URL) \
	--buildFuture \
	--minify

ci-preview-build: setup build-cue preview-build algolia

run-link-checker:
	htmltest

run-external-link-checker:
	htmltest --conf ./.htmltest.external.yml

# Check links without rebuilding the CUE sources. Optimal for fixing links locally.
check-links: production-build run-link-checker

# Search
algolia:
	yarn run algolia-config
	yarn run algolia-index

# Useful for locally debugging issues that arise only on the deployed production site
run-production-site-locally:
	make setup build-cue production-build
	python -m http.server 8000 --directory ./public --bind 127.0.0.1

# Full production build without Algolia updates (for debugging, link checking, etc.)
local-production-build: clean setup build-cue production-build run-link-checker
