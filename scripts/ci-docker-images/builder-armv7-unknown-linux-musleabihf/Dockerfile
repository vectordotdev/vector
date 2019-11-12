ARG RUST_VERSION=1.39.0
# The version of musl should be the same as the one Rust is compiled with.
# This version can be found in this file:
# https://github.com/rust-lang/rust/blob/master/src/ci/docker/scripts/musl-toolchain.sh.
ARG MUSL_VERSION=1.1.22
# We use Clang and libc++ from LLVM. The commit should match the one
# used by Rust. For each Rust version it can be found here
# https://github.com/rust-lang/rust/tree/$RUST_VERSION/src as the commit hash of the
# submodule "llvm-project".
ARG LLVM_GIT_COMMIT=8adf9bdccfefb8d03f0e8db3b012fb41da1580a4
# Used by libc++. Programs compiled with this version should work with other
# kernel versions too. See https://wiki.gentoo.org/wiki/Linux-headers#FAQ.
ARG LINUX_HEADERS_VERSION=5.3.10
# OpenSSL needs to be compiled against MUSL too, so we install it from source.
ARG OPENSSL_VERSION=1.1.0l
ARG TARGET=armv7-unknown-linux-musleabihf

# Locations for produced tools and libraries.
ARG RUST_PREFIX=/opt/rust
ARG CLANG_PREFIX=/opt/clang
ARG LIBS_PREFIX=/opt/libs

# Independent components are built in different stages, so that
# change of individual arguments would rebuild only affected stages.

# LLVM source tree is shared by both clang and libs builder stages.
FROM ubuntu:18.04 AS llvm-source
RUN apt-get update && apt-get install -y git
ENV SRC_DIR=/src
WORKDIR $SRC_DIR
ARG LLVM_GIT_COMMIT
RUN git clone https://github.com/rust-lang/llvm-project.git && \
  cd llvm-project && \
  git checkout $LLVM_GIT_COMMIT && \
  rm -rf .git
ENV LLVM_DIR=$SRC_DIR/llvm-project

# Clang
FROM llvm-source AS clang-builder
RUN apt-get update && apt-get -y install build-essential ninja-build cmake python3-distutils
ARG CLANG_PREFIX
WORKDIR $LLVM_DIR
RUN mkdir build && \
  cd build && \
  cmake \
  -DLLVM_ENABLE_PROJECTS="clang;lld" \
  -DLLVM_TARGETS_TO_BUILD="ARM" \
  -DLLVM_INSTALL_BINUTILS_SYMLINKS=ON \
  -DCMAKE_BUILD_TYPE=release \
  -DCMAKE_INSTALL_PREFIX=$CLANG_PREFIX \
  -G Ninja \
  ../llvm && \
  ninja install

# Libs
FROM llvm-source AS libs-builder

ARG CLANG_PREFIX
COPY --from=clang-builder $CLANG_PREFIX $CLANG_PREFIX
ENV PATH="$CLANG_PREFIX/bin:$PATH"

# Get all sources at first, so that changes in compilation
# flags would not redownload them.
RUN apt-get update && apt-get -y install curl xz-utils

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

ARG OPENSSL_VERSION
RUN curl --proto '=https' --tlsv1.2 -sSf \
  https://www.openssl.org/source/openssl-$OPENSSL_VERSION.tar.gz| \
  tar xzf -
ENV OPENSSL_DIR=$SRC_DIR/openssl-$OPENSSL_VERSION

# Install build dependencies.
RUN apt-get update && apt-get -y install rsync ninja-build cmake python3-distutils gcc-arm-linux-gnueabihf

ARG TARGET
ARG LIBS_PREFIX

# Compile Clang runtime (https://compiler-rt.llvm.org).
# We need crtbegin.o, crtend.o, and libclang_rt.builtins.a from there.
RUN apt-get install -y g++-arm-linux-gnueabihf
WORKDIR $LLVM_DIR/compiler-rt
ENV CC=arm-linux-gnueabihf-gcc
ENV CXX=arm-linux-gnueabihf-g++
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
  ninja install

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
ENV CC=clang
ENV CFLAGS="-target $TARGET -nostdinc -isystem $LIBS_PREFIX/include"
ENV LDFLAGS="-fuse-ld=lld -static -nostdlib -nostartfiles"
RUN make -j$(nproc) headers_install \
  KBUILD_VERBOSE=1 \
  HOSTCC="/usr/bin/gcc" \
  ARCH="arm" \
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
  ninja install

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
  -DLIBCXXABI_INSTALL_HEADER_PREFIX=$LIBS_PREFIX/include/c++/v1 \
  -DLLVM_PATH=$LLVM_DIR \
  -G Ninja \
  .. && \
  ninja install

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
  ninja install

# Create wrappers for Clang that automatically use correct libraries and start files.
# See
# https://blogs.gentoo.org/gsoc2016-native-clang/2016/05/31/build-gnu-free-executables-with-clang/
# and
# https://renenyffenegger.ch/notes/development/languages/C-C-plus-plus/GCC/options/no/compare-nostartfiles-nodefaultlibs-nolibc-nostdlib.
ENV MUSL_CFLAGS="-target $TARGET -nostdinc -isystem $LIBS_PREFIX/include"
ENV MUSL_CXXFLAGS="$MUSL_CFLAGS -nostdinc++ -I$LIBS_PREFIX/include/c++/v1"

ENV MUSL_LDFLAGS="-fuse-ld=lld -static -nostdlib -nostartfiles -L$LIBS_PREFIX/lib -L$LIBS_PREFIX/lib/linux -lc++ -lc++abi -lunwind -lclang_rt.builtins-armhf -lc"
ENV MUSL_STARTFILES="$LIBS_PREFIX/lib/crt1.o $LIBS_PREFIX/lib/linux/clang_rt.crtbegin-armhf.o $LIBS_PREFIX/lib/linux/clang_rt.crtend-armhf.o"

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
RUN ln -s $LIBS_PREFIX/bin/musl-cc $LIBS_PREFIX/bin/arm-linux-musleabihf-gcc
RUN ln -s $LIBS_PREFIX/bin/musl-cc $LIBS_PREFIX/bin/musl-gcc
RUN ln -s $LIBS_PREFIX/bin/musl-c++ $LIBS_PREFIX/bin/c++
RUN ln -s $LIBS_PREFIX/bin/musl-c++ $LIBS_PREFIX/bin/g++
RUN ln -s $LIBS_PREFIX/bin/musl-c++ $LIBS_PREFIX/bin/arm-linux-musleabihf-g++
RUN ln -s $LIBS_PREFIX/bin/musl-c++ $LIBS_PREFIX/bin/musl-g++

RUN ln -s $CLANG_PREFIX/bin/ar $LIBS_PREFIX/bin/arm-linux-musleabihf-ar

# Use stdatomic.h header provided by Clang because it is not present in musl.
RUN ln -s $CLANG_PREFIX/lib/clang/9.0.0/include/stdatomic.h $LIBS_PREFIX/include/stdatomic.h

# Some build systems hardcode -lstdc++ linker flag on all Linux systems.
# Because our linker is already configured to link with libc++ instead,
# we can just provide dummy libstdc++ to be compatible with GNU systems.
# For example, macOS provides similar experience, where
# `clang++ -o program program.cpp -lstdc++` would still link to libc++.
RUN echo > dummy.c && \
  $LIBS_PREFIX/bin/cc -c dummy.c && \
  $CLANG_PREFIX/bin/ar cr $LIBS_PREFIX/lib/libstdc++.a dummy.o && \
  rm dummy.c dummy.o

# Install OpenSSL
WORKDIR $OPENSSL_DIR
# OpenSSL tries to use secure_getenv function when compiled with MUSL here
# https://github.com/openssl/openssl/blob/master/crypto/getenv.c. However, this
# function was added to MUSL only in 1.23 release, while Rust still uses 1.22.
# So until there is a Rust version using MUSL 1.23 we need to resort to patching
# OpenSSL to disallow usage of unsupported secure_getenv function.
COPY openssl-disable-secure-getenv.patch .
RUN patch -p1 < openssl-disable-secure-getenv.patch
ENV CC=$LIBS_PREFIX/bin/musl-cc
ENV CXX=$LIBS_PREFIX/bin/musl-c++
ENV CFLAGS=""
ENV CXXFLAGS=""
ENV LDFLAGS=""
ENV CROSS_COMPILE=""
RUN ./Configure --prefix=$LIBS_PREFIX linux-generic32 no-shared && \
  make install

# The actual builder.
FROM ubuntu:18.04

# Install Rust using rustup.
RUN apt-get update && apt-get install -y curl
ARG RUST_PREFIX
ARG RUST_VERSION
ENV RUSTUP_HOME=$RUST_PREFIX
ENV CARGO_HOME=$RUST_PREFIX
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
  sh -s -- -y --default-toolchain $RUST_VERSION
ENV PATH="$RUST_PREFIX/bin:$PATH"
RUN rustup target add armv7-unknown-linux-musleabihf

ARG CLANG_PREFIX
COPY --from=clang-builder $CLANG_PREFIX $CLANG_PREFIX
ENV PATH="$CLANG_PREFIX/bin:$PATH"

ARG LIBS_PREFIX
COPY --from=libs-builder $LIBS_PREFIX $LIBS_PREFIX
ENV PATH="$LIBS_PREFIX/bin:$PATH"

RUN apt-get update && apt-get install -y --install-recommends build-essential cmake pkg-config openssh-client git

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

# Set up pkg-config.
ENV PKG_CONFIG_PATH=$LIBS_PREFIX/lib/pkgconfig
ENV PKG_CONFIG_ALLOW_CROSS=1

# Set up environment variables for Rust bindgen
# See https://github.com/rust-lang/rust-bindgen/issues/1229#issuecomment-473493753
ENV BINDGEN_EXTRA_CLANG_ARGS="--sysroot=/opt/libs -target armv7-unknown-linux-musleabihf"

# Set env vars for the scripts/build-archive.sh file
ENV TARGET=armv7-unknown-linux-musleabihf

RUN apt-get update && apt-get install -y qemu-user
