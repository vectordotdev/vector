FROM ubuntu:19.10

RUN apt-get update && apt-get install -y build-essential git libgraph-easy-perl
RUN git clone https://github.com/lindenb/makefile2graph && \
  cd makefile2graph && \
  git checkout 656a5a996e2aa70db8c9f0bb506003a24f3666cb && \
  make && \
  make install && \
  cd .. && \
  rm -rf makefile2graph
