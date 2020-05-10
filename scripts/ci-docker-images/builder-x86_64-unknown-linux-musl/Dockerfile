# We use compiler-rt, libunwind, and libc++ from LLVM.
ARG LLVM_VERSION=9.0.0
ARG LLVM_GIT_COMMIT=71fe7ec06b85f612fc0e4eb4134c7a7d0f23fac5
# The version of musl should be the same as the one Rust is compiled with.
# This version can be found in this file:
# https://github.com/rust-lang/rust/blob/master/src/ci/docker/scripts/musl-toolchain.sh.
# Exampel: "1.1.22".
ARG MUSL_VERSION=1.1.22

# LLVM target triple for Linux and MUSL. Full list of supported triples
# can be found here:
# https://forge.rust-lang.org/release/platform-support.html.
# Example: "armv7-unknown-linux-musleabihf".
ARG TARGET="x86_64-unknown-linux-musl"
# Target architecture for LLVM
ARG LLVM_TARGET="X86"
# Target architecture for Linux kernel headres
ARG LINUX_TARGET="x86"
# We also need GCC for the target architecture to compile compier-rt. Because
# GCC target triples are different from LLVM, we need to specify it explicitly.
# Example: "arm-linux-gnueabihf".
ARG GNU_TARGET=""

ARG LTO=""

# Used by libc++. Programs compiled with this version should work with other
# kernel versions too. See https://wiki.gentoo.org/wiki/Linux-headers#FAQ.
ARG LINUX_HEADERS_VERSION=5.3.10

# Locations for produced tools and libraries.
ARG RUST_PREFIX=/opt/rust
ARG CLANG_PREFIX=/usr
ARG LIBS_PREFIX=/opt/libs

# Libs
FROM ubuntu:19.10 AS libs-builder

ARG CLANG_PREFIX
RUN apt-get update && apt-get install -y llvm clang lld

# Get all sources at first, so that changes in arguments
# flags would not redownload them.
RUN apt-get update && apt-get -y install curl xz-utils

ARG LLVM_VERSION
RUN curl --proto '=https' --tlsv1.2 -sSfL \
  https://github.com/llvm/llvm-project/archive/llvmorg-$LLVM_VERSION.tar.gz | \
  tar xzf -
ENV LLVM_DIR $SRC_DIR/llvm-project-llvmorg-$LLVM_VERSION

ARG MUSL_VERSION
RUN curl --proto '=https' --tlsv1.2 -sSf \
  https://www.musl-libc.org/releases/musl-$MUSL_VERSION.tar.gz | \
  tar xzf -
ENV MUSL_DIR $SRC_DIR/musl-$MUSL_VERSION

ARG LINUX_HEADERS_VERSION
RUN curl --proto '=https' --tlsv1.2 -sSf \
  https://cdn.kernel.org/pub/linux/kernel/v5.x/linux-$LINUX_HEADERS_VERSION.tar.xz | \
  tar xJf -
ENV LINUX_DIR $SRC_DIR/linux-$LINUX_HEADERS_VERSION

# Install build dependencies.
RUN apt-get update && apt-get -y install rsync ninja-build cmake python3-distutils clang llvm lld

ARG TARGET
ARG CLANG_PREFIX
ARG LIBS_PREFIX

# Compile Clang runtime (https://compiler-rt.llvm.org).
# We need crtbegin.o, crtend.o, and libclang_rt.builtins.a from there.
# Because Clang toolchain is not fully bootstrapped yet (no standard library),
# it is necessary to use GCC to compile it.
ARG GNU_TARGET
RUN apt-get install -y gcc${GNU_TARGET:+-${GNU_TARGET}} g++${GNU_TARGET:+-${GNU_TARGET}}
WORKDIR $LLVM_DIR/compiler-rt
ENV CC=${GNU_TARGET:+${GNU_TARGET}-}gcc
ENV CXX=${GNU_TARGET:+${GNU_TARGET}-}g++
ENV CFLAGS=""
ENV CXXFLAGS=""
ENV LDFLAGS=""
RUN mkdir build && \
  cd build && \
  cmake \
    -DCMAKE_BUILD_TYPE=release \
    -DLLVM_CONFIG_PATH=$CLANG_PREFIX/bin/llvm-config \
		-DCOMPILER_RT_DEFAULT_TARGET_TRIPLE=$TARGET \
    -DCOMPILER_RT_BUILD_SANITIZERS=OFF \
    -DCOMPILER_RT_BUILD_LIBFUZZER=OFF \
    -DCOMPILER_RT_BUILD_XRAY=OFF \
    -DCOMPILER_RT_BUILD_PROFILE=OFF \
    -DCMAKE_INSTALL_PREFIX=$LIBS_PREFIX \
    -G Ninja \
    .. && \
  cmake --build . --target install

# Compile musl.
WORKDIR $MUSL_DIR
ENV CC=clang
ENV CFLAGS="-target $TARGET -nostdinc -isystem $LIBS_PREFIX/include -D_Noreturn="
ENV LDFLAGS="-fuse-ld=lld -static -nostdlib -nostartfiles"
ENV CROSS_COMPILE=$CLANG_PREFIX/bin/
RUN cd $MUSL_DIR && \
  mkdir build && \
  cd build && \
  ../configure \
  --target=$TARGET \
  --prefix=$LIBS_PREFIX \
  --disable-shared \
  --disable-gcc-wrapper && \
  make -j$(nproc) install

# Install Linux headers.
WORKDIR $LINUX_DIR
ARG LINUX_TARGET
ENV CC=clang
ENV CFLAGS="-target $TARGET -nostdinc -isystem $LIBS_PREFIX/include"
ENV LDFLAGS="-fuse-ld=lld -static -nostdlib -nostartfiles"
RUN make -j$(nproc) headers_install \
  KBUILD_VERBOSE=1 \
  HOSTCC="/usr/bin/gcc" \
  ARCH="$LINUX_TARGET" \
  CC="$CC" \
  INSTALL_HDR_PATH=$LIBS_PREFIX

# Compile libc++ with musl using Clang as the compiler. See
# https://blogs.gentoo.org/gsoc2016-native-clang/2016/05/05/build-a-freestanding-libcxx/
# for explanations.

# libunwind
WORKDIR $LLVM_DIR/libunwind
ENV CC=clang
ENV CXX=clang++
ENV CFLAGS="-target $TARGET -nostdinc -isystem $LIBS_PREFIX/include"
ENV CXXFLAGS="$CFLAGS -nostdinc++ -I$LLVM_DIR/libcxx/include"
ENV LDFLAGS="-fuse-ld=lld -static -nostdlib -nostartfiles -L$LIBS_PREFIX/lib -lc"
RUN mkdir build && \
  cd build && \
  cmake \
  -DCMAKE_BUILD_TYPE=release \
  -DLIBUNWIND_ENABLE_SHARED=OFF \
  -DLIBUNWIND_INSTALL_PREFIX=$LIBS_PREFIX/ \
  -DLLVM_PATH=$LLVM_DIR \
  -G Ninja \
  .. && \
  cmake --build . --target install

# libc++abi
WORKDIR $LLVM_DIR/libcxxabi
ENV CC=clang
ENV CXX=clang++
ENV CFLAGS="-target $TARGET -nostdinc -isystem $LIBS_PREFIX/include"
ENV CXXFLAGS="$CFLAGS -nostdinc++ -I$LLVM_DIR/libcxx/include"
ENV LDFLAGS="-fuse-ld=lld -static -nostdlib -nostartfiles -L$LIBS_PREFIX/lib -lunwind -lc"
RUN mkdir build && \
  cd build && \
  cmake \
  -DCMAKE_BUILD_TYPE=release \
  -DLIBCXXABI_ENABLE_SHARED=OFF \
  -DLIBCXXABI_USE_LLVM_UNWINDER=ON \
  -DLIBCXXABI_LIBUNWIND_PATH=$LLVM_DIR/libunwind \
  -DLIBCXXABI_LIBCXX_INCLUDES=$LLVM_DIR/libcxx/include \
  -DLIBCXXABI_INSTALL_PREFIX=$LIBS_PREFIX/ \
  -DLLVM_PATH=$LLVM_DIR \
  -G Ninja \
  .. && \
  cmake --build . --target install

# libc++
WORKDIR $LLVM_DIR/libcxx
ENV CC=clang
ENV CXX=clang++
ENV CFLAGS="-target $TARGET -nostdinc -isystem $LIBS_PREFIX/include"
ENV CXXFLAGS="$CFLAGS -nostdinc++ -I$LIBS_PREFIX/include/c++/v1"
ENV LDFLAGS="-fuse-ld=lld -static -nostdlib -nostartfiles -L$LIBS_PREFIX/lib -lc++abi -lunwind -lc"
RUN mkdir build && \
  cd build && \
  cmake \
  -DCMAKE_BUILD_TYPE=release \
  -DLIBCXX_ENABLE_SHARED=OFF \
  -DLIBCXX_HAS_MUSL_LIBC=ON \
  -DLIBCXX_HAS_GCC_S_LIB=OFF \
  -DLIBCXX_CXX_ABI=libcxxabi \
  -DLIBCXX_CXX_ABI_INCLUDE_PATHS=$LLVM_DIR/libcxxabi/include \
  -DLIBCXX_CXX_ABI_LIBRARY_PATH=$LIBS_PREFIX \
  -DLIBCXX_INSTALL_PREFIX=$LIBS_PREFIX/ \
  -DLIBCXX_INSTALL_HEADER_PREFIX=$LIBS_PREFIX/ \
  -DLLVM_PATH=$LLVM_DIR \
  -G Ninja \
  .. && \
  cmake --build . --target install

# Create wrappers for Clang that automatically use correct libraries and start files.
# See
# https://blogs.gentoo.org/gsoc2016-native-clang/2016/05/31/build-gnu-free-executables-with-clang/
# and
# https://renenyffenegger.ch/notes/development/languages/C-C-plus-plus/GCC/options/no/compare-nostartfiles-nodefaultlibs-nolibc-nostdlib.
ENV MUSL_CFLAGS="-target $TARGET -nostdinc -isystem $LIBS_PREFIX/include"
ENV MUSL_CXXFLAGS="$MUSL_CFLAGS -nostdinc++ -I$LIBS_PREFIX/include/c++/v1"

ENV MUSL_LDFLAGS="-fuse-ld=lld -static -nostdlib -nostartfiles -L$LIBS_PREFIX/lib -L$LIBS_PREFIX/lib/linux -lc++ -lc++abi -lunwind -l$(basename /opt/libs/lib/linux/libclang_rt.builtins* | tail -c+4 | head -c-3) -lc"
ENV MUSL_STARTFILES="$LIBS_PREFIX/lib/crt1.o $(find $LIBS_PREFIX/lib/linux -iname 'clang_rt.crtbegin*.o') $(find $LIBS_PREFIX/lib/linux -iname 'clang_rt.crtend*.o')"

RUN mkdir -p $LIBS_PREFIX/bin
RUN echo \
  "#!/bin/sh\n"\
  "case \"\$@\" in *-shared*);; *-nostdlib*);; *) STARTFILES=\"$MUSL_STARTFILES\";; esac\n"\
  "$CLANG_PREFIX/bin/clang -Qunused-arguments $MUSL_CFLAGS \$@ \$STARTFILES $MUSL_LDFLAGS\n"\
  "exit \$?" > $LIBS_PREFIX/bin/musl-cc
RUN echo \
  "#!/bin/sh\n"\
  "case \"\$@\" in *-shared*);; *-nostdlib*);; *) STARTFILES=\"$MUSL_STARTFILES\";; esac\n"\
  "$CLANG_PREFIX/bin/clang++ -Qunused-arguments $MUSL_CXXFLAGS \$@ \$STARTFILES $MUSL_LDFLAGS\n"\
  "exit \$?" > $LIBS_PREFIX/bin/musl-c++
RUN chmod +x $LIBS_PREFIX/bin/*

# At this point a fully functional C++ compiler that is able to produce
# static binaries linked with musl and libc++ is bootstrapped.
# It can be used by calling musl-c++ (or musl-cc for C) executable.
# However, we need to also create generic aliases to make it possible to
# use it as a drop-in replacement for the system-wide GCC.
RUN ln -s $LIBS_PREFIX/bin/musl-cc $LIBS_PREFIX/bin/cc
RUN ln -s $LIBS_PREFIX/bin/musl-cc $LIBS_PREFIX/bin/gcc
RUN if [ -n "$GNU_TARGET" ]; then ln -s $LIBS_PREFIX/bin/musl-cc $LIBS_PREFIX/bin/$(echo $GNU_TARGET | sed 's/gnu/musl/g')-gcc; fi
RUN ln -s $LIBS_PREFIX/bin/musl-cc $LIBS_PREFIX/bin/musl-gcc
RUN ln -s $LIBS_PREFIX/bin/musl-c++ $LIBS_PREFIX/bin/c++
RUN ln -s $LIBS_PREFIX/bin/musl-c++ $LIBS_PREFIX/bin/g++
RUN if [ -n "$GNU_TARGET" ]; then ln -s $LIBS_PREFIX/bin/musl-c++ $LIBS_PREFIX/bin/$(echo $GNU_TARGET | sed 's/gnu/musl/g')-g++; fi
RUN ln -s $LIBS_PREFIX/bin/musl-c++ $LIBS_PREFIX/bin/musl-g++

RUN ln -s $CLANG_PREFIX/bin/llvm-ar $LIBS_PREFIX/bin/ar
RUN ln -s $CLANG_PREFIX/bin/llvm-strip $LIBS_PREFIX/bin/strip
RUN ln -s $CLANG_PREFIX/bin/ld.lld $LIBS_PREFIX/bin/ld

# Use some headers provided by Clang because they are not present in musl.
RUN ln -s $CLANG_PREFIX/lib/clang/9.0.0/include/stdatomic.h $LIBS_PREFIX/include/stdatomic.h
RUN ln -s $CLANG_PREFIX/lib/clang/9.0.0/include/emmintrin.h $LIBS_PREFIX/include/emmintrin.h
RUN ln -s $CLANG_PREFIX/lib/clang/9.0.0/include/xmmintrin.h $LIBS_PREFIX/include/xmmintrin.h
RUN ln -s $CLANG_PREFIX/lib/clang/9.0.0/include/mmintrin.h $LIBS_PREFIX/include/mmintrin.h
RUN ln -s $CLANG_PREFIX/lib/clang/9.0.0/include/mm_malloc.h $LIBS_PREFIX/include/mm_malloc.h

# Some build scripts hardcode -lstdc++ linker flag on all Linux systems.
# Because our linker is already configured to link with libc++ instead,
# we can just provide dummy libstdc++ to be compatible with GNU systems.
# For example, macOS provides similar experience, where
# `clang++ -o program program.cpp -lstdc++` would still link to libc++.
RUN echo > dummy.c && \
  $LIBS_PREFIX/bin/cc -c dummy.c && \
  $CLANG_PREFIX/bin/llvm-ar cr $LIBS_PREFIX/lib/libstdc++.a dummy.o && \
  rm dummy.c dummy.o

# The actual builder.
FROM ubuntu:19.10

# Install Rust using rustup.
RUN apt-get update && apt-get install -y curl
ARG TARGET
ARG RUST_PREFIX
ENV RUSTUP_HOME=$RUST_PREFIX/rustup
ENV CARGO_HOME=$RUST_PREFIX/cargo
COPY rust-toolchain /tmp/
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
  sh -s -- -y --profile minimal --default-toolchain $(cat /tmp/rust-toolchain) --target $TARGET
ENV PATH="$RUSTUP_HOME/bin:$CARGO_HOME/bin:$PATH"

RUN apt-get update && apt-get install -y llvm clang lld

ARG LIBS_PREFIX
COPY --from=libs-builder $LIBS_PREFIX $LIBS_PREFIX
ENV PATH="$LIBS_PREFIX/bin:$PATH"

# Essential tools needed for compiling native dependencies
RUN apt-get update && apt-get install -y --install-recommends build-essential cmake git

# Because the system GCC and binutils are shadowed by aliases, we need to
# instruct Cargo and cc crate to use GCC on the host system. They are used to
# compile dependencies of build scripts.
RUN echo \
  "#!/bin/sh\n"\
  "/usr/bin/gcc -B/usr/bin \$@\n"\
  "exit \$?" > $LIBS_PREFIX/bin/gnu-cc && chmod +x $LIBS_PREFIX/bin/gnu-cc
RUN echo \
  "#!/bin/sh\n"\
  "/usr/bin/g++ -B/usr/bin \$@\n"\
  "exit \$?" > $LIBS_PREFIX/bin/gnu-c++ && chmod +x $LIBS_PREFIX/bin/gnu-c++

ENV CC_x86_64_unknown_linux_gnu=gnu-cc
ENV CXX_x86_64_unknown_linux_gnu=gnu-c++
ENV LD_x86_64_unknown_linux_gnu=/usr/bin/ld
ENV AR_x86_64_unknown_linux_gnu=/usr/bin/ar

ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=gnu-cc

# Set up environment variables for Rust bindgen
# See https://github.com/rust-lang/rust-bindgen/issues/1229#issuecomment-473493753
ENV BINDGEN_EXTRA_CLANG_ARGS="--sysroot=/opt/libs -target $TARGET"

# Set default Cargo target
RUN mkdir -p ~/.cargo
RUN printf "[build]\ntarget = \"$TARGET\"\n" > ~/.cargo/config

# In case of non-native target, install QEMU-user to enable running `cargo test`
RUN if [ -n "$GNU_TARGET"]; then apt-get update && apt-get install -y qemu-user; fi

# Add dependencies for running tests
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y tzdata ca-certificates

ENV TARGET="$TARGET"

# Install `cargo-deb from Git until
# https://github.com/mmstick/cargo-deb/pull/112 is published on crates.io
RUN cargo install --git https://github.com/mmstick/cargo-deb --rev cc879930c06fa22e99f2839bad620ac0d0da879e cargo-deb

RUN apt-get install -y rename cmark-gfm

# Install journalctl
RUN apt-get install -y systemd
