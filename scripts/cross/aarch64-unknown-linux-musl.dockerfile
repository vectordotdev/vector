FROM rustembedded/cross:aarch64-unknown-linux-musl

ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="-C link-arg=-lgcc -C link-arg=-lstdc++"