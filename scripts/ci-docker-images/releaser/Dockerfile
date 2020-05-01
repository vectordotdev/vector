FROM ruby:2.7

# Install the git repository to ensure we get the latest version.
# This is important for determining the current Vector version number.
RUN apt-get update && \
  apt-get install -y software-properties-common
RUN apt-key adv --keyserver keyserver.ubuntu.com --recv-keys A1715D88E1DF1F24

RUN apt-get update && \
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

# Use UTF-8 since this is required to process release notes
ENV LC_ALL C.UTF-8

RUN gem install rake

# Docker is used to upload Vector docker images
RUN apt-get remove docker docker-engine docker.io
RUN apt-get update && apt-get install -y docker.io

# Install gems to reduce setup time
RUN gem install bundler -v '~> 2.1.4'
COPY scripts/Gemfile Gemfile
COPY scripts/Gemfile.lock Gemfile.lock
RUN bundle install
RUN rm Gemfile
