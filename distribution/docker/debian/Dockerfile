ARG ALPINE_IMAGE=docker.io/alpine:3.16.0
ARG DEBIAN_IMAGE=docker.io/debian:bullseye-slim

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
COPY vector-*-unknown-linux-gnu*.tar.gz .
RUN mkdir vector \
    && tar zxvf vector-*-"$(cat /etc/apk/arch)"-unknown-linux-gnu*.tar.gz \
    -C vector --strip-components=2

# Prepare Vector in the staging root filesystem
RUN cp vector/bin/* rootfs/usr/bin/ \
  && cp -r vector/config/* rootfs/etc/vector/

#-------------------------------------------------------------------------------
FROM ${DEBIAN_IMAGE}

# Set image labels
ARG VECTOR_VERSION
LABEL org.opencontainers.image.source="https://github.com/vectordotdev/vector" \
  org.opencontainers.image.title="Vector (Debian variant)" \
  org.opencontainers.image.url="https://vector.dev/" \
  org.opencontainers.image.version="${VECTOR_VERSION}"

# Copy root filesystem from staging
COPY --from=staging /staging/rootfs/ /

# Prepare a user/group for running Vector
ARG VECTOR_USER=vector
ARG VECTOR_GROUP=vector
RUN apt-get update \
    && apt-get upgrade -y \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        tini \
        tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system \
        --shell /sbin/nologin \
        --user-group "${VECTOR_USER}" \
    && chown -R "${VECTOR_USER}":"${VECTOR_GROUP}" /etc/vector /var/lib/vector

# Configure the image entrypoint
ENTRYPOINT ["tini", "--", "docker-entrypoint.sh"]
CMD ["vector"]
