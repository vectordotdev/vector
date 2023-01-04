FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main-centos@sha256:d83ab33e6500234a2d4cb03cf543030fedea02de9fc3e5755b394973e203540d

COPY scripts/cross/bootstrap-centos.sh scripts/cross/entrypoint-centos.sh scripts/environment/install-protoc.sh /
RUN /bootstrap-centos.sh && bash /install-protoc.sh

ENTRYPOINT [ "/entrypoint-centos.sh" ]
