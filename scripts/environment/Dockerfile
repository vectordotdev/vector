FROM docker.io/ubuntu:20.04 AS ENVIRONMENT
ENV DEBIAN_FRONTEND=noninteractive \
    TZ='America/New York' \
    PATH=/root/.cargo/bin:$PATH \
    LANG=en_US.UTF-8 \
    LANGUAGE=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8

# Normal updates
RUN echo $TZ > /etc/timezone && \
    apt update --yes && \
    apt upgrade --yes

# Dependencies
RUN apt install --yes \
    build-essential \
    locales \
    docker.io \
    curl \
    ruby-bundler \
    nodejs \
    gnupg2

# Additional setup
RUN locale-gen en_US.UTF-8 && \
    dpkg-reconfigure locales

# Install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y

# Yarn
RUN curl -sS https://dl.yarnpkg.com/debian/pubkey.gpg | apt-key add - && \
    echo "deb https://dl.yarnpkg.com/debian/ stable main" | tee /etc/apt/sources.list.d/yarn.list && \
    apt update --yes && \
    apt install --yes yarn

# Setup the env
RUN mkdir -p /git/timberio/vector/{scripts,website,scripts/environment}
ADD . /git/timberio/vector/
ADD website/package.json website/yarn.lock /git/timberio/vector/website/

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
