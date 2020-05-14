FROM amazonlinux:2

RUN yum update -y && \
  yum install -y \
  ca-certificates \
  curl \
  git \
  systemd \
  tar

# Configure temp dir
ENV TMPDIR=/docker-tmp
RUN mkdir -p "$TMPDIR" && chmod 0777 "$TMPDIR"
VOLUME [ "$TMPDIR" ]
