FROM docker.io/ubuntu:24.04@sha256:d1e2e92c075e5ca139d51a140fff46f84315c0fdce203eab2807c7e495eff4f9
ENV DEBIAN_FRONTEND=noninteractive \
    TZ='America/New York' \
    PATH=/root/.cargo/bin:/root/.local/bin/:$PATH \
    LANG=en_US.UTF-8 \
    LANGUAGE=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8 \
    CROSS_DOCKER_IN_DOCKER=true

# Container junk
RUN echo $TZ > /etc/timezone

RUN apt-get update && apt-get install -y git

WORKDIR /git/vectordotdev/vector

# Setup the env
COPY scripts/environment/*.sh scripts/environment/
COPY scripts/environment/npm-tools/ scripts/environment/npm-tools/
RUN ./scripts/environment/bootstrap-ubuntu-24.04.sh

# Setup the toolchain
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
