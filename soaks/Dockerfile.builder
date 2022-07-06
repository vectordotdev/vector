FROM docker.io/rust:1.61-bullseye@sha256:816886827dbc6c248ebb29119fc9d8b2b74418a1a113463beae0c0b8460c5c5f as builder
RUN apt-get update && \
    apt-get dist-upgrade -y && \
    apt-get -y --no-install-recommends install build-essential git clang cmake libclang-dev \
    libsasl2-dev libstdc++-10-dev libssl-dev libxxhash-dev zlib1g-dev zlib1g && \
		rm -rf /var/lib/apt/lists/*

# Build mold, a fast linker
RUN git clone https://github.com/rui314/mold.git && cd mold && git checkout v1.2.1 && make -j"$(nproc)" && make install

# Smoke test
RUN ["/usr/local/bin/mold", "--version"]
