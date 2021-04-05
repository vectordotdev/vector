setup:
	yarn

build-cue:
	./scripts/build-cue.sh

serve: setup build-cue
	hugo server \
	--buildDrafts \
	--buildFuture

ci-production-build: setup build-cue
	hugo \
	--minify

ci-preview-build: setup build-cue
	hugo \
	--baseURL $(DEPLOY_PRIME_URL) \
	--minify
