FROM japaric/x86_64-unknown-freebsd

RUN apt-get update && \
	apt-get install -y \
	gcc-4.7 \
	g++-4.7 \
	libssl-dev


RUN rm /usr/bin/gcc
RUN ln -s /usr/bin/gcc-4.7 /usr/bin/gcc

RUN ln -s /usr/bin/g++-4.7 /usr/bin/g++
