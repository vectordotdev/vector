FROM ubuntu:18.04

# Install the git repository to ensure we get the latest version.
# This is important for determining the current Vector version number.
RUN apt-get update && \
  apt-get install -y software-properties-common
RUN add-apt-repository -y ppa:git-core/ppa

RUN apt-get update && \
  apt-get upgrade -y && \
  apt-get update && \  
  apt-get install -y \
  build-essential \
  curl \
  git \
  libssl-dev \
  locales \
  python3-pip \
  rename \
  ruby-full \
  zlib1g-dev \
	wget

# AWS CLI
RUN pip3 install --upgrade awscli

# Grease is used to upload packages to Github
RUN wget https://github.com/timberio/grease/releases/download/v1.0.1/grease-1.0.1-linux-amd64.tar.gz
RUN tar -xvf grease-1.0.1-linux-amd64.tar.gz
ENV PATH="$PATH:/grease/bin"
ENV LC_ALL en_US.UTF-8

RUN gem install rake

# Docker is used to upload Vector docker images
RUN apt-get remove docker docker-engine docker.io
RUN apt-get install -y docker.io

ENV LANGUAGE en_US.UTF-8
ENV LANG en_US.UTF-8
ENV LC_ALL en_US.UTF-8
RUN locale-gen "en_US.UTF-8"
RUN dpkg-reconfigure locales

# Install gems to reduce setup time
RUN gem install bundler
COPY checker/Gemfile Gemfile
RUN bundle install
RUN rm Gemfile