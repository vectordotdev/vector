FROM rust:1.33.0 as build

WORKDIR /usr/src/vector

# Fetch external dependencies.
RUN mkdir -p src && touch src/lib.rs
COPY benches benches
COPY lib lib
COPY Cargo.toml Cargo.lock ./
RUN cargo fetch --locked

COPY src src
COPY build.rs build.rs
RUN cargo build -p vector --bin vector --frozen --release

FROM alpine as runtime
WORKDIR /vector
COPY --from=build /usr/src/vector/target/release/vector ./vector
RUN ls -la /vector
ENTRYPOINT ["/vector/vector"]
