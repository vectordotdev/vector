FROM docker.io/rustembedded/cross:x86_64-unknown-linux-musl

RUN apt update
# needed by onig_sys
RUN apt install -y \
      libclang1 \
      llvm
