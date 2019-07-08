FROM ubuntu

# Install the git repository to ensure we get the latest version.
# This is important for determining the current Vector version number.
RUN apt-get update
RUN apt-get install -y software-properties-common
RUN add-apt-repository ppa:git-core/ppa

RUN apt-get update && \
  apt-get upgrade -y && \
  apt-get update
  
RUN apt-get install -y \
  build-essential \
  curl \
  git \
  python3-pip \
  rename \
  ruby-full \
	wget

# AWS CLI
RUN pip3 install --upgrade awscli

# Grease is used to upload packages to Github
RUN wget https://github.com/timberio/grease/releases/download/v1.0.1/grease-1.0.1-linux-amd64.tar.gz
RUN tar -xvf grease-1.0.1-linux-amd64.tar.gz
ENV PATH="$PATH:/grease/bin"

# Package Cloud is used to host deb, rpm, and other packages
ENV LC_ALL en_US.UTF-8

RUN gem install rake
RUN gem install package_cloud

# Docker is used to upload Vector docker images
RUN apt-get remove docker docker-engine docker.io
RUN apt-get install -y docker.io