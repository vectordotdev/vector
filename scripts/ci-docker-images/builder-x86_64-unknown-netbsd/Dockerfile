# Note
#
# This docker image does not successfully compile vector! This Docker image
# successfully builds but it does not succeed in compiling Vector. This is
# due to the fact that leveldb and rdkafka do not compile properly.

FROM timberiodev/vector-builder-base-x86_64-unknown-netbsd:latest

RUN apt-get update
RUN apt-get install -y --no-install-recommends \
  apt-transport-https \
  curl \
  g++ \
  libleveldb-dev \
  libsnappy-dev \
  software-properties-common

RUN curl -L https://packages.confluent.io/deb/5.2/archive.key | apt-key add -
RUN add-apt-repository "deb [arch=amd64] https://packages.confluent.io/deb/5.2 stable main"
RUN apt-get update
RUN apt-get install -y librdkafka-dev
ENV CARGO_FEATURE_DYNAMIC_LINKING=true
ENV PKG_CONFIG_PATH=/usr/local/lib/x86_64-linux-gnu/pkgconfig:/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig:/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig
ENV PKG_CONFIG_ALLOW_CROSS=true