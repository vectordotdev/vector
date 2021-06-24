clean:
	rm -rf public resources

setup:
	yarn

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

local-production-build: setup build-cue production-build run-link-checker

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

# Check links without rebuilding the CUE sources. Optimal for fixing links locally.
check-links: production-build run-link-checker

# Search
algolia:
	yarn run algolia-config
	yarn run algolia-index

# Useful for debugging issues that arise only on the deployed production site
run-production-site-locally:
	make setup build-cue production-build
	python -m http.server 8000 --directory ./public --bind 127.0.0.1
