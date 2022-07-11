ARG ALPINE_IMAGE=docker.io/alpine:3.16.0
ARG DISTROLESS_CC_IMAGE=gcr.io/distroless/cc-debian11

#-------------------------------------------------------------------------------
FROM ${ALPINE_IMAGE} AS staging

# Set a well-known working directory for staging
WORKDIR /staging

# Prepare a staging root filesystem
RUN mkdir -p \
  rootfs/etc/vector \
  rootfs/usr/bin \
  rootfs/var/lib/vector

# Copy and unpack Vector distribution
COPY vector-*-unknown-linux-gnu*.tar.gz .
RUN mkdir vector \
    && tar zxvf vector-*-"$(cat /etc/apk/arch)"-unknown-linux-gnu*.tar.gz \
    -C vector --strip-components=2

# Prepare Vector in the staging root filesystem
RUN cp vector/bin/* rootfs/usr/bin/ \
  && cp -r vector/config/* rootfs/etc/vector/

#-------------------------------------------------------------------------------
FROM ${ALPINE_IMAGE}

# Set image labels
ARG VECTOR_VERSION
LABEL org.opencontainers.image.source="https://github.com/vectordotdev/vector" \
  org.opencontainers.image.title="Vector (Distroless libc variant)" \
  org.opencontainers.image.url="https://vector.dev/" \
  org.opencontainers.image.version="${VECTOR_VERSION}"

# Copy root filesystem from staging
COPY --from=staging /staging/rootfs/ /

# Configure the image entrypoint
ENTRYPOINT ["vector"]
