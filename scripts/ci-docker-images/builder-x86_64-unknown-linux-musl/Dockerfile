# Nightly Rust is used because at the moment only nightly uses recent enough
# LLVM version that supports libc++ with musl. It can be changed to a stable
# version after Rust 1.38 release.
ARG RUST_VERSION=nightly-2019-07-29
# The version of musl should be the same as the one Rust is compiled with.
# This version can be found in this file:
# https://github.com/rust-lang/rust/blob/master/src/ci/docker/scripts/musl-toolchain.sh.
ARG MUSL_VERSION=1.1.22
# We use Clang and libc++ from LLVM. The commit should match the one
# used by Rust. For each Rust version it can be found here
# https://github.com/rust-lang/rust/tree/master/src as the commit hash of the
# submodule "llvm-project".
ARG LLVM_GIT_COMMIT=9b64ca5b7e1e3583978f9ac8af6d93b220a13d90
# Used by libc++. Programs compiled with this version should work with other
# kernel versions too. See https://wiki.gentoo.org/wiki/Linux-headers#FAQ.
ARG LINUX_HEADERS_VERSION=5.2.2
# LibreSSL uses more modern build system than OpenSSL, which makes it better
# suited for use with musl.
ARG LIBRESSL_VERSION=2.9.2

# LTO type is set as a build arg because it is used for compilation of the
# libraries and configuration of the linker. Possible values: "lto", "lto=thin",
# and "". See https://doc.rust-lang.org/rustc/linker-plugin-lto.html and
# http://blog.llvm.org/2016/06/thinlto-scalable-and-incremental-lto.html.
#
# It is a linker-level LTO that uses output of rustc produced with
# -C linker-plugin-lto option. That is different from LLVM LTO inside rustc
# that can be activated by -C lto.
ARG LTO="lto"

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
    -DLLVM_TARGETS_TO_BUILD="X86" \
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

ARG LIBRESSL_VERSION
RUN curl --proto '=https' --tlsv1.2 -sSf \
    https://ftp.openbsd.org/pub/OpenBSD/LibreSSL/libressl-$LIBRESSL_VERSION.tar.gz | \
    tar xzf -
ENV LIBRESSL_DIR=$SRC_DIR/libressl-$LIBRESSL_VERSION

# Install build dependencies.
RUN apt-get update && apt-get -y install curl ninja-build cmake python3-distutils

ARG LIBS_PREFIX
ARG LTO

# Compile musl.
WORKDIR $MUSL_DIR
ENV CC=clang
ENV CFLAGS="${LTO:+-f$LTO} -nostdinc -isystem $LIBS_PREFIX/include"
ENV LDFLAGS="-fuse-ld=lld -static -nostdlib -nostartfiles"
RUN cd $MUSL_DIR && \
  mkdir build && \
  cd build && \
    ../configure \
      --prefix=$LIBS_PREFIX \
      --disable-shared \
      --disable-gcc-wrapper && \
  make -j$(nproc) install

# Install Linux headers.
WORKDIR $LINUX_DIR
ENV CC=clang
ENV CFLAGS="${LTO:+-f$LTO} -nostdinc -isystem $LIBS_PREFIX/include"
ENV LDFLAGS="-fuse-ld=lld -static -nostdlib -nostartfiles $LIBS_PREFIX/lib/crt1.o -L$LIBS_PREFIX/lib -lc"
RUN make -j$(nproc) headers_install \
  HOSTCC="$CC" \
  CC="$CC" \
  HOSTCFLAGS="$CFLAGS" \
  HOSTLDFLAGS="$LDFLAGS" \
  INSTALL_HDR_PATH=$LIBS_PREFIX

# Compile Clang runtime (https://compiler-rt.llvm.org).
# We need crtbegin.o, crtend.o, and libclang_rt.builtins.a from there.
# It depends on libc, so it has to be built after musl.
# LTO flags are not added here because compiler-rt doesn't support LTO.
WORKDIR $LLVM_DIR/compiler-rt
ENV CC=clang
ENV CXX=clang++
ENV CFLAGS="-nostdinc -isystem $LIBS_PREFIX/include"
ENV CXXFLAGS="$CFLAGS -nostdinc++"
ENV LDFLAGS="-fuse-ld=lld -static -nostdlib -nostartfiles $LIBS_PREFIX/lib/crt1.o -L$LIBS_PREFIX/lib -lc"
RUN mkdir build && \
  cd build && \
  cmake \
    -DCMAKE_BUILD_TYPE=release \
    -DLLVM_CONFIG_PATH=$CLANG_PREFIX/bin/llvm-config \
    -DCOMPILER_RT_BUILD_SANITIZERS=OFF \
    -DCOMPILER_RT_BUILD_LIBFUZZER=OFF \
    -DCOMPILER_RT_BUILD_XRAY=OFF \
    -DCOMPILER_RT_BUILD_PROFILE=OFF \
    -DCMAKE_INSTALL_PREFIX=$LIBS_PREFIX \
    -G Ninja \
    .. && \
  ninja install

# Compile libc++ with musl using Clang as the compiler. See
# https://blogs.gentoo.org/gsoc2016-native-clang/2016/05/05/build-a-freestanding-libcxx/
# for explanations.

# libunwind
WORKDIR $LLVM_DIR/libunwind
ENV CC=clang
ENV CXX=clang++
ENV CFLAGS="${LTO:+-f$LTO} -nostdinc -isystem $LIBS_PREFIX/include"
ENV CXXFLAGS="$CFLAGS -nostdinc++ -I$LLVM_DIR/libcxx/include"
ENV LDFLAGS="-fuse-ld=lld ${LTO:+-f$LTO} -static -nostdlib -nostartfiles -L$LIBS_PREFIX/lib -lc"
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
ENV CFLAGS="${LTO:+-f$LTO} -nostdinc -isystem $LIBS_PREFIX/include"
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
ENV CFLAGS="${LTO:+-f$LTO} -nostdinc -isystem $LIBS_PREFIX/include"
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
ENV MUSL_CFLAGS="${LTO:+-f$LTO} -nostdinc -isystem $LIBS_PREFIX/include"
ENV MUSL_CXXFLAGS="$MUSL_CFLAGS -nostdinc++ -I$LIBS_PREFIX/include/c++/v1"

ENV MUSL_LDFLAGS="-fuse-ld=lld -static -nostdlib -nostartfiles -L$LIBS_PREFIX/lib -L$LIBS_PREFIX/lib/linux -lc++ -lc++abi -lunwind -lclang_rt.builtins-x86_64 -lc"
ENV MUSL_STARTFILES="$LIBS_PREFIX/lib/crt1.o $LIBS_PREFIX/lib/linux/clang_rt.crtbegin-x86_64.o $LIBS_PREFIX/lib/linux/clang_rt.crtend-x86_64.o"

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
RUN echo \
"#!/bin/sh\n"\
"$CLANG_PREFIX/bin/ld.lld -L$LIBS_PREFIX/lib -L$LIBS_PREFIX/lib/linux \$@\n"\
"exit \$?" > $LIBS_PREFIX/bin/musl-ld
RUN chmod +x $LIBS_PREFIX/bin/*

# At this point a fully functional C++ compiler that is able to produce
# static binaries linked with musl and libc++ is bootstrapped.
# It can be used by calling musl-c++ (or musl-cc for C) executable.
# However, we need to also create generic aliases to make it possible to
# use it as a drop-in replacement for the system-wide GCC.
RUN ln -s $LIBS_PREFIX/bin/musl-cc $LIBS_PREFIX/bin/cc
RUN ln -s $LIBS_PREFIX/bin/musl-cc $LIBS_PREFIX/bin/gcc
RUN ln -s $LIBS_PREFIX/bin/musl-cc $LIBS_PREFIX/bin/musl-gcc
RUN ln -s $LIBS_PREFIX/bin/musl-c++ $LIBS_PREFIX/bin/c++
RUN ln -s $LIBS_PREFIX/bin/musl-c++ $LIBS_PREFIX/bin/g++
RUN ln -s $LIBS_PREFIX/bin/musl-c++ $LIBS_PREFIX/bin/musl-g++
RUN ln -s $LIBS_PREFIX/bin/musl-ld $LIBS_PREFIX/bin/ld

# Some build systems hardcode -lstdc++ linker flag on all Linux systems.
# Because our linker is already configured to link with libc++ instead,
# we can just provide dummy libstdc++ to be compatible with GNU systems.
# For example, macOS provides similar experience, where
# `clang++ -o program program.cpp -lstdc++` would still link to libc++.
RUN echo > dummy.c && \
  $CLANG_PREFIX/bin/clang ${LTO:+-f$LTO} -c dummy.c && \
  $CLANG_PREFIX/bin/ar cr $LIBS_PREFIX/lib/libstdc++.a dummy.o && \
  rm dummy.c dummy.o

# Install LibreSSL.
WORKDIR $LIBRESSL_DIR
# Rust crate openssl-sys supports only released versions of LibreSSL.
# In order to be able to use the latest stable release with musl,
# we need to backport unreleased changes from this PR:
# https://github.com/libressl-portable/portable/pull/529/files.
RUN sed -i \
  's/#error "Cannot emulate getprogname"/return program_invocation_short_name;/g' \
  crypto/compat/getprogname_linux.c
ENV CC=$LIBS_PREFIX/bin/musl-cc
ENV CXX=$LIBS_PREFIX/bin/musl-c++
ENV CFLAGS=""
ENV CXXFLAGS=""
ENV LDFLAGS=""
RUN mkdir build && \
  cd build && \
  cmake \
    -DCMAKE_BUILD_TYPE=release \
    -DCMAKE_INSTALL_PREFIX=$LIBS_PREFIX \
    -DLIBRESSL_APPS=OFF \
    -DLIBRESSL_TESTS=OFF \
    -G Ninja \
    .. && \
  ninja install

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
RUN rustup target add x86_64-unknown-linux-musl

ARG CLANG_PREFIX
COPY --from=clang-builder $CLANG_PREFIX $CLANG_PREFIX
ENV PATH="$CLANG_PREFIX/bin:$PATH"

ARG LIBS_PREFIX
COPY --from=libs-builder $LIBS_PREFIX $LIBS_PREFIX
ENV PATH="$LIBS_PREFIX/bin:$PATH"

RUN apt-get update && apt-get install -y build-essential git cmake pkg-config

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

# Set up linker and enable LTO plugin in rustc if necessary.
ARG LTO
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="${LTO:+-Clinker-plugin-lto -Clink-arg=-O3 }-Clink-arg=${LTO:+-f$LTO}"

# Set up pkg-config.
ENV PKG_CONFIG_PATH=$LIBS_PREFIX/lib/pkgconfig
ENV PKG_CONFIG_ALLOW_CROSS=1

# Set env vars for the scripts/build-archive.sh file
ENV TARGET=x86_64-unknown-linux-musl