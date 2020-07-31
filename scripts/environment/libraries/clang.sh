#! /usr/bin/env bash
set -e -o verbose

# TARGET=x86-64-linux-musl HOST=x86-64-linux-gnu ./scripts/environment/libraries/clang.sh

# The GNU_TARGET should be the is the target platform
GNU_TARGET=${GNU_TARGET}
TARGET=${TARGET:?"The TARGET should be the guest platform. (Eg 'aarch-linux-musl')"}
# We don't need these...
# shellcheck disable=SC2034
CFLAGS=""
# shellcheck disable=SC2034
CXXFLAGS=""
LDFLAGS=""

CLANG_PREFIX=${CLANG_PREFIX:=/usr}
LLVM_VERSION=${LLVM_VERSION:=9.0.0}
CC=${GNU_TARGET:+${GNU_TARGET}-}gcc
CXX=${GNU_TARGET:+${GNU_TARGET}-}g++

SRC_DIR=${SRC_DIR:=/opt/libs}
LLVM_DIR=${SRC_DIR}/llvm-project-llvmorg-${LLVM_VERSION}
LLVM_GIT_COMMIT=${LLVM_GIT_COMMIT:=71fe7ec06b85f612fc0e4eb4134c7a7d0f23fac5} #... What is this for?

apt-get install --yes \
  llvm \
  clang \
  lld \
  curl \
  xz-utils \
  cmake \
  rsync \
  ninja-build \
  python3-distutils \
  lld \
  "gcc${GNU_TARGET:+-${GNU_TARGET}}" \
  "g++${GNU_TARGET:+-${GNU_TARGET}}"

mkdir -p ${SRC_DIR}
cd ${SRC_DIR}

curl --proto '=https' --tlsv1.2 -sSfL \
  https://github.com/llvm/llvm-project/archive/llvmorg-${LLVM_VERSION}.tar.gz | \
  tar --extract --gzip --verbose

cd ${LLVM_DIR}/compiler-rt

mkdir -p build
cd build

cmake \
  -DCMAKE_BUILD_TYPE=release \
  -DLLVM_CONFIG_PATH=$CLANG_PREFIX/bin/llvm-config \
  -DCOMPILER_RT_DEFAULT_TARGET_TRIPLE="${TARGET}" \
  -DCOMPILER_RT_BUILD_SANITIZERS=OFF \
  -DCOMPILER_RT_BUILD_LIBFUZZER=OFF \
  -DCOMPILER_RT_BUILD_XRAY=OFF \
  -DCOMPILER_RT_BUILD_PROFILE=OFF \
  -DCMAKE_INSTALL_PREFIX="${LIBS_PREFIX}" \
  -G Ninja \
  ..

cmake --build . --target install