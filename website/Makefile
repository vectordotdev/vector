serve:
	hugo server \
	--buildDrafts \
	--buildFuture

ci-production-build:
	hugo \
	--minify

ci-preview-build:
	hugo \
	--baseURL $(DEPLOY_PRIME_URL) \
	--minify
