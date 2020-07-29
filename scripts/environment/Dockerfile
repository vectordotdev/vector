ARG BASEIMAGE=docker.io/ubuntu:20.04

FROM ${BASEIMAGE} AS ENVIRONMENT
ARG BOOTSTRAPPER=scripts/environment/bootstraps/ubuntu-20.04.sh

ENV DEBIAN_FRONTEND=noninteractive \
    TZ='America/New York' \
    PATH=/root/.cargo/bin:/root/.local/bin/:$PATH \
    LANG=en_US.UTF-8 \
    LANGUAGE=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8

# Container junk
RUN echo $TZ > /etc/timezone

# Setup the env
RUN mkdir -p /git/timberio/vector/{scripts,website,scripts/environment}
ADD ${BOOTSTRAPPER} /git/timberio/vector/${BOOTSTRAPPER}
ADD scripts/environment/prepare.sh \
    /git/timberio/vector/scripts/environment/

# Setup the toolchain
WORKDIR /git/timberio/vector
RUN ${BOOTSTRAPPER}

ADD scripts/Gemfile scripts/Gemfile.lock \
    /git/timberio/vector/scripts/
ADD website/package.json website/yarn.lock \
    /git/timberio/vector/website/
ADD rust-toolchain \
    /git/timberio/vector/
RUN ./scripts/environment/prepare.sh

# Declare volumes
VOLUME /vector
VOLUME /vector/target
VOLUME /root/.cargo

# Prepare for use
ADD ./scripts/environment/entrypoint.sh /
ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "bash" ]
