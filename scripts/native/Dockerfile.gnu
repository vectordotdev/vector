# Match the existing native GNU release job's maintained glibc 2.28 environment.
FROM almalinux:8@sha256:2d0f2eb2a83cc09dbc2eb92fade4dbe8f709628149e81aa00aec70630406603b

RUN dnf install -y --setopt=install_weak_deps=False \
      gcc gcc-c++ make cmake perl perl-IPC-Cmd \
      openssl-devel cyrus-sasl-devel zlib-devel pkgconfig clang \
      git tar gzip xz which findutils curl unzip \
    && dnf clean all

ARG RUST_VERSION
ARG RUST_HOST
ARG RUST_TARGET
ENV CARGO_HOME=/opt/cargo RUSTUP_HOME=/opt/rustup
ENV PATH="/opt/cargo/bin:${PATH}"
RUN curl --retry 5 -fsSL "https://static.rust-lang.org/rustup/dist/${RUST_HOST}/rustup-init" -o /tmp/rustup-init \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal \
      --default-host "${RUST_HOST}" --default-toolchain "${RUST_VERSION}" \
      --target "${RUST_TARGET}" \
    && rm /tmp/rustup-init
ARG INSTALL_CLIPPY=false
RUN if [ "${INSTALL_CLIPPY}" = true ]; then rustup component add clippy; fi

COPY scripts/environment/install-protoc.sh /tmp/install-protoc.sh
RUN bash /tmp/install-protoc.sh /usr/local/bin && rm /tmp/install-protoc.sh

# EL8's keyutils development headers otherwise make vendored Cyrus SASL silently
# omit its GSSAPI plugin. Keep the same setting as the native GNU publish job.
ENV ac_cv_header_keyutils_h=no
ENV PROTOC=/usr/local/bin/protoc CARGO_INCREMENTAL=0
RUN git config --global --add safe.directory /work
WORKDIR /work
