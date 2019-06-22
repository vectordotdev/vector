FROM debian:9-slim as build

RUN apt-get update && \
	apt-get install -y \
	zlib1g-dev \
	build-essential \
	libssl-dev \
	curl \
	pkg-config

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
ENV PATH="$PATH:/root/.cargo/bin"

WORKDIR /vector

# Prepare
RUN mkdir -p src
RUN touch src/lib.rs

# Copy source
COPY Cargo.toml Cargo.lock ./
COPY lib lib
COPY benches benches
COPY build.rs build.rs
COPY proto proto
COPY src src

# Build
RUN cargo fetch
RUN cargo build --release --frozen

# New layer
FROM debian:9-slim as runtime

WORKDIR /vector

COPY config config
RUN mkdir -p bin
COPY --from=build /vector/target/release/vector bin
RUN mkdir -p data
RUN apt-get update && apt-get install -y \
	libssl-dev && \
	rm -rf /var/lib/apt/lists/* && \
	rm -rf /var/cache/apt/*

# Finalize
ENTRYPOINT ["/vector/bin/vector", "--config", "/vector/config/vector.toml"]
