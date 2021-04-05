build-cue:
	./scripts/build-cue.sh

serve: build-cue
	hugo server \
	--buildDrafts \
	--buildFuture

ci-production-build: build-cue
	hugo \
	--minify

ci-preview-build: build-cue
	hugo \
	--baseURL $(DEPLOY_PRIME_URL) \
	--minify
