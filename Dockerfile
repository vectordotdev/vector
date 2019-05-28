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

# Fetch external dependencies.
RUN mkdir -p src && touch src/lib.rs

COPY Cargo.toml Cargo.lock ./
COPY lib lib
COPY benches benches
COPY build.rs build.rs
COPY proto proto

RUN cargo fetch

COPY src src
RUN cargo build --release --frozen

FROM debian:9-slim as runtime

WORKDIR /vector
COPY --from=build /vector/target/release/vector /vector

RUN apt-get update && apt-get install -y \
	libssl-dev && \
	rm -rf /var/lib/apt/lists/* && \
	rm -rf /var/cache/apt/*

ENTRYPOINT ["/vector/vector"]
