ARG CROSS_VERSION=0.2.5
ARG TARGET=x86_64-unknown-linux-musl

FROM ghcr.io/cross-rs/${TARGET}:${CROSS_VERSION}

# Common steps for all targets
COPY scripts/cross/bootstrap-ubuntu.sh /
COPY scripts/environment/install-protoc.sh /
RUN /bootstrap-ubuntu.sh && bash /install-protoc.sh

# Relocate libstdc++ for musl targets that need it (TODO: investigate if still required)
RUN if [ "$TARGET" = "arm-unknown-linux-musleabi" ]; then \
      LIBSTDC=/usr/local/arm-linux-musleabi/lib/libstdc++.a; \
    elif [ "$TARGET" = "armv7-unknown-linux-musleabihf" ]; then \
      LIBSTDC=/usr/local/arm-linux-musleabihf/lib/libstdc++.a; \
    elif [ "$TARGET" = "aarch64-unknown-linux-musl" ]; then \
      LIBSTDC=/usr/local/aarch64-linux-musl/lib/libstdc++.a; \
    elif [ "$TARGET" = "x86_64-unknown-linux-musl" ]; then \
      LIBSTDC=/usr/local/x86_64-linux-musl/lib/libstdc++.a; \
    fi && \
    if [ -n "${LIBSTDC:-}" ]; then \
      mkdir -p /lib/native-libs && \
      cp "$LIBSTDC" /lib/native-libs/; \
    fi
