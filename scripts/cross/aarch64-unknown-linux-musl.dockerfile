FROM rustembedded/cross:aarch64-unknown-linux-musl

ENV RUSTC="scripts/cross/wrappers/musl_rustc.sh"
