FROM debian:bookworm-slim

ARG HUGO_VERSION=0.154.5
ARG CUE_VERSION=0.16.1
ARG NODE_VERSION=24

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates git make \
    && rm -rf /var/lib/apt/lists/*

# Hugo extended (required for Sass/PostCSS processing)
# uname -m: x86_64 -> amd64, aarch64 -> arm64
RUN ARCH=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/') && \
    curl -fsSL https://github.com/gohugoio/hugo/releases/download/v${HUGO_VERSION}/hugo_extended_${HUGO_VERSION}_linux-${ARCH}.tar.gz \
    | tar -xz -C /usr/local/bin hugo

# CUE CLI
RUN ARCH=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/') && \
    curl -fsSL https://github.com/cue-lang/cue/releases/download/v${CUE_VERSION}/cue_v${CUE_VERSION}_linux_${ARCH}.tar.gz \
    | tar -xz -C /usr/local/bin cue

# Node.js + Yarn
RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_VERSION}.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && npm install -g yarn \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app/website

EXPOSE 1313

# generate-vrl-docs (requires Rust/vdev) must be run on the host first.
# Call cue directly instead of scripts/cue.sh to avoid its `git rev-parse` dependency.
ENTRYPOINT ["/bin/sh", "-c", \
  "if [ -z \"$(find cue/reference/remap/functions -name '*.cue' 2>/dev/null | head -1)\" ]; then \
     echo 'ERROR: cue/reference/remap/functions/ is empty.'; \
     echo 'Run on the host first: make generate-vrl-docs'; \
     exit 1; \
   fi && \
   yarn && \
   cp -f /app/Cargo.lock data/cargo-lock.toml && \
   rm -f data/docs.json && find cue -name '*.cue' | xargs cue export --all-errors --outfile data/docs.json && \
   yarn config-examples && \
   hugo server --bind 0.0.0.0 --port 1313 --buildDrafts --buildFuture --environment development"]
