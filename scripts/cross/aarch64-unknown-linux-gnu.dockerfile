FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main-centos@sha256:88f269991f2e5882fb9339c2cf90947e4d9ee7ecaccf437c1d640833bb36ab65

COPY scripts/cross/bootstrap-centos.sh scripts/cross/entrypoint-centos.sh scripts/environment/install-protoc.sh /
RUN /bootstrap-centos.sh && bash /install-protoc.sh

ENTRYPOINT [ "/entrypoint-centos.sh" ]
