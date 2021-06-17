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

ci-production-build: setup build-cue production-build

# Preview site
preview-build:
	hugo \
	--baseURL $(DEPLOY_PRIME_URL) \
	--buildFuture \
	--minify

ci-preview-build: setup build-cue preview-build

run-link-checker:
	htmltest

# Check links without rebuilding the CUE sources. Optimal for fixing links locally.
check-links: production-build run-link-checker

# Check links in the CI environment
check-links-ci: ci-production-build run-link-checker
