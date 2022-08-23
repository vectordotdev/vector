FROM docker.io/rust:1.62-bullseye@sha256:5709afe04a23c0a447b02077b3ab8ff3d9458e80e1f9898e40873df36a34981b as builder
RUN apt-get update && \
    apt-get dist-upgrade -y && \
    apt-get -y --no-install-recommends install build-essential git clang cmake libclang-dev \
    libsasl2-dev libstdc++-10-dev libssl-dev libxxhash-dev zlib1g-dev zlib1g && \
		rm -rf /var/lib/apt/lists/*

# Build mold, a fast linker
RUN git clone https://github.com/rui314/mold.git && cd mold && git checkout v1.2.1 && make -j"$(nproc)" && make install

# also update scripts/cross/bootstrap-ubuntu.sh
ENV PROTOC_VERSION=3.19.4
ENV PROTOC_ZIP=protoc-${PROTOC_VERSION}-linux-x86_64.zip

RUN \
  curl -fsSL https://github.com/protocolbuffers/protobuf/releases/download/v$PROTOC_VERSION/$PROTOC_ZIP \
    --output "$TEMP/$PROTOC_ZIP" && \
  unzip "$TEMP/$PROTOC_ZIP" bin/protoc -d "$TEMP" && \
  chmod +x "$TEMP"/bin/protoc && \
  mv --force --verbose "$TEMP"/bin/protoc /usr/bin/protoc

# Smoke test
RUN ["/usr/local/bin/mold", "--version"]
