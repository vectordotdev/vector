FROM ghcr.io/cross-rs/aarch64-unknown-linux-musl:0.2.5

COPY scripts/cross/bootstrap-ubuntu.sh scripts/environment/install-protoc.sh /
RUN /bootstrap-ubuntu.sh && bash /install-protoc.sh

# Stick `libstdc++` somewhere it can be found other than it's normal location, otherwise we end up using the wrong version
# of _other_ libraries, which ultimately just breaks linking. We'll set `/lib/native-libs` as a search path in `.cargo/config.toml`.
RUN mkdir -p /lib/native-libs && cp /usr/local/aarch64-linux-musl/lib/libstdc++.a /lib/native-libs/
