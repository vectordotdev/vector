
FROM ubuntu:18.04

RUN apt-get update && \
  apt-get upgrade -y

RUN apt-get update && \
  apt-get install -y \
  build-essential \
  curl \
  git \
  libssl-dev \
  pkg-config \
  ruby-full \
  zlib1g-dev

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
ENV PATH="$PATH:/root/.cargo/bin"
RUN rustup component add rustfmt

# We need ruby to check for documentation changes :)
ENV LC_ALL C.UTF-8
RUN gem install bundler
COPY checker/Gemfile Gemfile
RUN bundle install
RUN rm Gemfile