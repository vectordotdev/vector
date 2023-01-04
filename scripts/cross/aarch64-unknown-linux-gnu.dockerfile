FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main-centos@sha256:8a446c469a1bc009c3e50a5b36a61c7c5a66e4fe96c56b4ea1b44e635a784c45

COPY scripts/cross/bootstrap-centos.sh scripts/cross/entrypoint-centos.sh scripts/environment/install-protoc.sh /
RUN /bootstrap-centos.sh && bash /install-protoc.sh

ENTRYPOINT [ "/entrypoint-centos.sh" ]
