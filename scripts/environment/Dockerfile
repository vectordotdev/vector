FROM docker.io/ubuntu:20.04 AS ENVIRONMENT
ENV DEBIAN_FRONTEND=noninteractive \
    TZ='America/New York' \
    PATH=/root/.cargo/bin:/root/.local/bin/:$PATH \
    LANG=en_US.UTF-8 \
    LANGUAGE=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8 \
    CROSS_DOCKER_IN_DOCKER=true

# Container junk
RUN echo $TZ > /etc/timezone

# Setup the env
RUN mkdir -p /git/vectordotdev/vector/scripts/environment
ADD scripts/environment/bootstrap-ubuntu-20.04.sh /git/vectordotdev/vector/scripts/environment/
RUN ./git/vectordotdev/vector/scripts/environment/bootstrap-ubuntu-20.04.sh

# Setup the toolchain
WORKDIR /git/vectordotdev/vector
ADD scripts/environment/prepare.sh /git/vectordotdev/vector/scripts/environment/
ADD scripts/environment/setup-helm.sh /git/vectordotdev/vector/scripts/environment/
ADD scripts/environment/release-flags.sh /git/vectordotdev/vector/scripts/environment/
ADD scripts/Gemfile scripts/Gemfile.lock \
    /git/vectordotdev/vector/scripts/
ADD rust-toolchain.toml \
    /git/vectordotdev/vector/
RUN ./scripts/environment/prepare.sh
RUN ./scripts/environment/setup-helm.sh

# Declare volumes
VOLUME /vector
VOLUME /vector/target
VOLUME /root/.cargo

# Prepare for use
ADD ./scripts/environment/entrypoint.sh /
ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "bash" ]
