FROM koalaman/shellcheck:stable AS shellcheck

FROM debian:buster

RUN apt-get update \
  && apt-get install -y \
  git \
  && rm -rf /var/lib/apt/lists/*

COPY --from=shellcheck /bin/shellcheck /usr/local/bin/shellcheck

# Configure temp dir
ENV TMPDIR=/docker-tmp
RUN mkdir -p "$TMPDIR" && chmod 0777 "$TMPDIR"
VOLUME [ "$TMPDIR" ]
