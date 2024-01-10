FROM ghcr.io/cross-rs/x86_64-unknown-linux-gnu:0.2.5-centos

COPY scripts/cross/bootstrap-centos.sh scripts/cross/entrypoint-centos.sh scripts/environment/install-protoc.sh /
RUN /bootstrap-centos.sh && bash /install-protoc.sh

ENTRYPOINT [ "/entrypoint-centos.sh" ]
