FROM japaric/x86_64-unknown-linux-gnu:latest

RUN apt-get install -y python-software-properties

RUN apt-get update
RUN add-apt-repository ppa:ubuntu-toolchain-r/test
RUN apt-get update && \
	apt-get install -y \
	zlib1g-dev \
	build-essential \
	libssl-dev \
	gcc-4.7 \
	g++-4.7

RUN rm /usr/bin/gcc
RUN ln -s /usr/bin/gcc-4.7 /usr/bin/gcc

RUN rm /usr/bin/g++
RUN ln -s /usr/bin/g++-4.7 /usr/bin/g++
