FROM ghcr.io/cross-rs/x86_64-unknown-linux-gnu:0.2.5

RUN apt-get update && apt-get install -y \
    apt-transport-https \
    ca-certificates \
    wget \
    gnupg \
    software-properties-common \
    && rm -rf /var/lib/apt/lists/*

RUN sed -i \
    -e 's|http://[^/]*/ubuntu|https://mirrors.tuna.tsinghua.edu.cn/ubuntu|g' \
    /etc/apt/sources.list

RUN apt-get update && apt-get install -y \
    liblldpctl-dev \
    pkg-config \
    build-essential \
    libc6-dev \
    libclang-dev \
    clang \
    && rm -rf /var/lib/apt/lists/*

COPY scripts/cross/bootstrap-ubuntu.sh scripts/environment/install-protoc.sh /
RUN /bootstrap-ubuntu.sh && bash /install-protoc.sh