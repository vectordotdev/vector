ARG ALPINE_IMAGE=docker.io/alpine:3.16.0

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

# Prepare a user/group for running Vector
ARG VECTOR_USER=vector
ARG VECTOR_GROUP=vector
RUN apk --no-cache add \
        ca-certificates \
        tini \
        tzdata \
    && addgroup -S "${VECTOR_GROUP}" \
    && adduser -S -H \
        -s /sbin/nologin \
        -G "${VECTOR_GROUP}" "${VECTOR_USER}" \
    && chown -R "${VECTOR_USER}":"${VECTOR_GROUP}" /etc/vector /var/lib/vector

# Configure the image entrypoint
ENTRYPOINT ["tini", "--", "docker-entrypoint.sh"]
CMD ["vector"]
