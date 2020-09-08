FROM rustembedded/cross:x86_64-unknown-linux-musl

ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="-C link-arg=-lstdc++"
