FROM debian:bullseye-slim

RUN apt-get update \
  && apt-get install -y \
  ca-certificates \
  tzdata \
  patchelf \
  && rm -rf /var/lib/apt/lists/*

ENTRYPOINT ["/usr/bin/vector"]
STOPSIGNAL SIGTERM

ARG VECTOR_LOG=debug,vector=trace
ENV VECTOR_LOG="$VECTOR_LOG"

COPY vector /usr/bin/vector

RUN set -x \
  && ls -la /usr/bin/vector \
  && ldd /usr/bin/vector \
  && patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 /usr/bin/vector \
  && ldd /usr/bin/vector \
  && /usr/bin/vector --version
