FROM docker.io/ubuntu:20.04 AS ENVIRONMENT
ENV DEBIAN_FRONTEND=noninteractive \
    TZ='America/New York' \
    PATH=/root/.cargo/bin:$PATH \
    LANG=en_US.UTF-8 \
    LANGUAGE=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8

# Normal updates
RUN echo $TZ > /etc/timezone

# Dependencies
ADD scripts/environment/bootstrap-ubuntu-20.04.sh /bootstrap-ubuntu-20.04.sh
RUN /bootstrap-ubuntu-20.04.sh

# Setup the env
RUN mkdir -p /git/timberio/vector/{scripts,website,scripts/environment}
ADD scripts/environment scripts/Gemfile scripts/Gemfile.lock \
    /git/timberio/vector/scripts/
ADD website/package.json website/yarn.lock \
    /git/timberio/vector/website/

# Setup the toolchain
WORKDIR /git/timberio/vector
RUN ./scripts/environment/prepare.sh

# Declare volumes
VOLUME /vector
VOLUME /vector/target
VOLUME /root/.cargo

# Prepare for use
ADD ./scripts/environment/entrypoint.sh /
ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "bash" ]
