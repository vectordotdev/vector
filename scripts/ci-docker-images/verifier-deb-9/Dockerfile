FROM debian:9

RUN apt-get update && \
  apt-get upgrade -y && \
  apt-get update
  
RUN apt-get install -y \
  ca-certificates \
  curl \
  git \
  systemd