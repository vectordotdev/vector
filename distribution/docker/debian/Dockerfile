FROM debian:9

RUN apt-get update && \
	apt-get install -y \
	zlib1g-dev \
	build-essential \
	libssl-dev \
	curl \
	pkg-config \
	git

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
ENV PATH="$PATH:/root/.cargo/bin"

RUN cargo install cargo-deb
