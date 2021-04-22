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
	--environment "production" \
	--minify

ci-production-build: setup build-cue production-build

# Preview site
preview-build:
	hugo \
	--baseURL $(DEPLOY_PRIME_URL) \
	--buildDrafts \
	--buildFuture \
	--minify

ci-preview-build: setup build-cue preview-build

check-links:
	htmltest
