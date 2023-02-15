ARG RUST_VERSION
FROM docker.io/rust:${RUST_VERSION}-slim-bullseye

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    cmake \
    curl \
    g++ \
    libclang1-9 \
    libsasl2-dev \
    libssl-dev \
    llvm-9 \
    openjdk-11-jdk \
    pkg-config \
    zlib1g-dev \
    unzip \
  && rm -rf /var/lib/apt/lists/*

# Link installed java runtime to default to avoid arch like amd64 or arm64
RUN ln -s $(readlink -f /usr/bin/java | sed "s:bin/java::") /usr/lib/jvm/default
ENV JAVA_HOME=/usr/lib/jvm/default

ENV HADOOP_VERSION 3.2.4
ENV HADOOP_URL https://www.apache.org/dist/hadoop/common/hadoop-$HADOOP_VERSION/hadoop-$HADOOP_VERSION.tar.gz

RUN set -x \
    && curl -fSL "$HADOOP_URL" -o /tmp/hadoop.tar.gz \
    && tar -xvf /tmp/hadoop.tar.gz -C /opt/ \
    && rm /tmp/hadoop.tar.gz*

ENV HADOOP_HOME=/opt/hadoop-$HADOOP_VERSION

RUN rustup run "${RUST_VERSION}" cargo install cargo-nextest --version 0.9.25 --locked

COPY scripts/environment/install-protoc.sh /
RUN bash /install-protoc.sh
