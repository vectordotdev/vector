ARG RUST_VERSION=${RUST_VERSION}
FROM docker.io/rust:${RUST_VERSION}-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    unixodbc \
    odbcinst \
    odbc-mariadb \
  && rm -rf /var/lib/apt/lists/*

RUN rustup run "${RUST_VERSION}" cargo install cargo-nextest --version 0.9.72 --locked
