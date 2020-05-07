FROM koalaman/shellcheck:stable AS shellcheck

FROM debian:buster

RUN apt-get update \
  && apt-get install -y \
      git \
  && rm -rf /var/lib/apt/lists/*

COPY --from=shellcheck /bin/shellcheck /usr/local/bin/shellcheck
