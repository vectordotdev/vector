FROM ubuntu

# Install the git repository to ensure we get the latest version.
# This is important for determining the current Vector version number.
RUN apt-get update
RUN apt-get install -y software-properties-common
RUN add-apt-repository ppa:git-core/ppa

RUN apt-get update
RUN apt-get upgrade -y
RUN apt-get install -y \
  build-essential \
  git \
  python3-pip \
  rename \
  ruby-full \
	wget

RUN gem install rake 
RUN pip3 install --upgrade awscli

# Grease is used to upload packages to Github
RUN wget https://github.com/timberio/grease/releases/download/v1.0.1/grease-1.0.1-linux-amd64.tar.gz
RUN tar -xvf grease-1.0.1-linux-amd64.tar.gz
ENV PATH="$PATH:/grease/bin"

# Package Cloud is used to host deb, rpm, and other packages
RUN gem install rake
RUN gem install package_cloud