ARG ALPINE_IMAGE=docker.io/alpine:3.15

#-------------------------------------------------------------------------------
FROM ${ALPINE_IMAGE} AS staging

# Set a well-known working directory for staging
WORKDIR /staging

# Prepare a staging root filesystem
RUN mkdir -p \
  rootfs/etc/vector \
  rootfs/usr/bin \
  rootfs/var/lib/vector

COPY docker-entrypoint.sh rootfs/usr/bin/docker-entrypoint.sh
COPY --from=docker.io/tianon/gosu:latest gosu rootfs/usr/bin/gosu

# Copy and unpack Vector distribution
COPY vector-*-unknown-linux-musl*.tar.gz .
RUN mkdir vector \
    && tar zxvf vector-*-"$(cat /etc/apk/arch)"-unknown-linux-musl*.tar.gz \
    -C vector --strip-components=2

# Prepare Vector in the staging root filesystem
RUN cp vector/bin/* rootfs/usr/bin/ \
  && cp -r vector/config/* rootfs/etc/vector/

#-------------------------------------------------------------------------------
FROM ${ALPINE_IMAGE}

# Set image labels
ARG VECTOR_VERSION
LABEL org.opencontainers.image.source="https://github.com/vectordotdev/vector" \
  org.opencontainers.image.title="Vector (Alpine variant)" \
  org.opencontainers.image.url="https://vector.dev/" \
  org.opencontainers.image.version="${VECTOR_VERSION}"

# Copy root filesystem from staging
COPY --from=staging /staging/rootfs/ /

RUN apk --no-cache add \
        ca-certificates \
        tini \
        tzdata \
    && addgroup -S vector \
    && adduser -S -H \
        -s /sbin/nologin \
        -G vector vector \
    && chown vector /etc/vector /var/lib/vector

# Configure the image entrypoint
ENTRYPOINT ["tini", "--", "docker-entrypoint.sh"]
CMD ["vector"]
