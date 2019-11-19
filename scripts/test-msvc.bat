set RUSTFLAGS=-Ctarget-feature=+crt-static
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
del rust-toolchain
cargo test --no-default-features --features "leveldb leveldb/leveldb-sys-3 rdkafka rdkafka/cmake_build"