FROM ubuntu

RUN apt-get update && \
	apt-get install -y \
	wget \
	git

RUN wget https://github.com/timberio/grease/releases/download/v1.0.1/grease-1.0.1-linux-amd64.tar.gz
RUN tar -xvf grease-1.0.1-linux-amd64.tar.gz

ENV PATH="$PATH:/grease/bin"
