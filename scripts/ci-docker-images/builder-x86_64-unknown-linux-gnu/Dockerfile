FROM centos:7 as builder
# This is formatted "$UID:$GID" by the docker-compose/scripts.
ARG USER

RUN yum install -y epel-release && \
    yum clean all && \
    yum makecache
RUN yum install -y \
        make openssl-devel cmake3 git \
        gcc gcc-c++ libstdc++-static sudo \
        perl-App-cpanminus && \
    yum clean all
RUN ln -s /usr/bin/cmake3 /usr/bin/cmake
RUN cpanm File::Rename \
 && rename --version

RUN cd /tmp && \
  git clone https://github.com/github/cmark-gfm && \
  cd cmark-gfm && \
  git checkout 0.29.0.gfm.0 && \
  make install INSTALL_PREFIX=/usr && \
  ldconfig && \
  cd .. && \
  rm -rf cmark-gfm && \
  cmark-gfm --version

RUN echo "%sudo         ALL = (ALL) NOPASSWD: ALL" >> /etc/sudoers.d/sudo
RUN useradd -m -G wheel \
    -u $(echo "${USER}" | cut -d: -f1) \
    # -g $(echo "${USER}" | cut -d: -f2) \
    runner

USER ${USER}
RUN curl https://sh.rustup.rs -sSf | sh -s -- --no-modify-path --default-toolchain none -y
ENV PATH=/home/runner/.cargo/bin:$PATH
RUN echo "export PATH=/home/runner/.cargo/bin:$PATH" >> ~/bashrc
ENV LIBRARY_PATH /usr/local/lib:$LIBRARY_PATH
ENV LD_LIBRARY_PATH /usr/local/lib:$LD_LIBRARY_PATH

RUN rustup update stable
RUN rustup run stable cargo install cargo-deb --version '^1.24.0'

ENTRYPOINT ["/bin/bash"]
