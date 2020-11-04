FROM rustembedded/cross:armv7-unknown-linux-musleabihf

ENV RUSTC="scripts/cross/wrappers/armv7_rustc.sh"
