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
COPY scripts/environment/*.sh /git/vectordotdev/vector/scripts/environment/
RUN cd git/vectordotdev/vector && ./scripts/environment/bootstrap-ubuntu-20.04.sh

# Setup the toolchain
WORKDIR /git/vectordotdev/vector
COPY scripts/Gemfile scripts/Gemfile.lock \
    /git/vectordotdev/vector/scripts/
COPY rust-toolchain.toml \
    /git/vectordotdev/vector/
RUN ./scripts/environment/prepare.sh && ./scripts/environment/setup-helm.sh

# Declare volumes
VOLUME /vector
VOLUME /vector/target
VOLUME /root/.cargo
VOLUME /root/.rustup

# Prepare for use
COPY ./scripts/environment/entrypoint.sh /
ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "bash" ]
