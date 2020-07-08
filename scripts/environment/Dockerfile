FROM docker.io/ubuntu:20.04 AS ENVIRONMENT
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
ADD scripts/environment/bootstrap-ubuntu-20.04.sh scripts/environment/prepare.sh \
    /git/timberio/vector/scripts/environment/

# Setup the toolchain
WORKDIR /git/timberio/vector
RUN ./scripts/environment/bootstrap-ubuntu-20.04.sh

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
