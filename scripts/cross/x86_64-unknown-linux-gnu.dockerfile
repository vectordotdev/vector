FROM docker.io/rustembedded/cross:x86_64-unknown-linux-gnu

COPY bootstrap-ubuntu.sh .
RUN ./bootstrap-ubuntu.sh

ENV LIBCLANG_PATH=/opt/rh/llvm-toolset-7/root/usr/lib64/ \
  LIBCLANG_STATIC_PATH=/opt/rh/llvm-toolset-7/root/usr/lib64/ \
  CLANG_PATH=/opt/rh/llvm-toolset-7/root/usr/bin/clang
