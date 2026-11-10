ARG TARGET=x86_64-unknown-linux-musl
ARG CROSS_DIGEST

FROM docker.io/ubuntu:24.04@sha256:24737cef17f4e22b1977df7d9a05e77c4e28b7da5b8911f8d8ccda136bf3573c AS trust-store

# Use Ubuntu's maintained CA set without upgrading the cross toolchain or target libc.
# Keep the CA package current, as in the runtime Dockerfiles.
# hadolint ignore=DL3008
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

FROM ghcr.io/cross-rs/${TARGET}@${CROSS_DIGEST}

# Replace both the source certificates and generated links before any downloads.
RUN rm -rf /usr/share/ca-certificates /etc/ssl/certs && mkdir -p /etc/ssl/certs
COPY --from=trust-store /usr/share/ca-certificates/ /usr/share/ca-certificates/
COPY --from=trust-store /etc/ca-certificates.conf /etc/ca-certificates.conf
RUN update-ca-certificates --fresh

# Common steps for all targets
COPY scripts/cross/bootstrap-ubuntu.sh /
COPY scripts/environment/install-protoc.sh /
RUN /bootstrap-ubuntu.sh && bash /install-protoc.sh

# Allow cmake to find pre-built dependencies (e.g. curl from curl-sys) that
# land in CMAKE_PREFIX_PATH rather than the cross sysroot.  The cross-rs
# toolchain.cmake sets CMAKE_FIND_ROOT_PATH_MODE_{LIBRARY,INCLUDE} to ONLY,
# which makes cmake ignore CMAKE_PREFIX_PATH entries outside the sysroot.
# Switching to BOTH lets FindCURL (and similar modules) locate libraries that
# Cargo crates like curl-sys build from source and expose via CMAKE_PREFIX_PATH.
RUN if [ -f /opt/toolchain.cmake ]; then \
      printf '\n# Allow finding pre-built cargo dependencies via CMAKE_PREFIX_PATH\nset(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY BOTH)\nset(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE BOTH)\n' >> /opt/toolchain.cmake; \
    fi

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
