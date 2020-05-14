FROM ubuntu:16.04

ARG DEBIAN_FRONTEND="noninteractive"

RUN apt-get update \
  && apt-get install -y \
  ca-certificates \
  curl \
  git \
  systemd \
  && rm -rf /var/lib/apt/lists/*

# Configure temp dir
ENV TMPDIR=/docker-tmp
RUN mkdir -p "$TMPDIR" && chmod 0777 "$TMPDIR"
VOLUME [ "$TMPDIR" ]
