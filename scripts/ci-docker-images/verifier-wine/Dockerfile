FROM ubuntu:18.04

RUN apt-get update && apt-get install -y unzip wine-stable

# Configure temp dir
ENV TMPDIR=/docker-tmp
RUN mkdir -p "$TMPDIR" && chmod 0777 "$TMPDIR"
VOLUME [ "$TMPDIR" ]
