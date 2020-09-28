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
RUN mkdir -p /git/timberio/vector/scripts
RUN mkdir -p /git/timberio/vector/scripts/environment
ADD scripts/environment/bootstrap-ubuntu-20.04.sh /git/timberio/vector/scripts/environment/
RUN ./git/timberio/vector/scripts/environment/bootstrap-ubuntu-20.04.sh

# Setup the toolchain
WORKDIR /git/timberio/vector
ADD scripts/environment/prepare.sh /git/timberio/vector/scripts/environment/
ADD scripts/Gemfile scripts/Gemfile.lock \
    /git/timberio/vector/scripts/
ADD rust-toolchain \
    /git/timberio/vector/
RUN ./scripts/environment/prepare.sh
ADD scripts/environment/.markdownlintrc /git/timberio/vector/scripts/environment/

# Setup Kubernetes
ADD scripts/environment/ci-setup-helm.sh /git/timberio/vector/scripts/environment/
RUN ./scripts/environment/ci-setup-helm.sh

# Declare volumes
VOLUME /vector
VOLUME /vector/target
VOLUME /root/.cargo

# Prepare for use
ADD ./scripts/environment/entrypoint.sh /
ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "bash" ]
