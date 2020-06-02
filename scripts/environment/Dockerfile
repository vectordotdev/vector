# Bootstrap
FROM docker.io/nixos/nix:2.3.4
RUN nix-channel --add https://nixos.org/channels/nixpkgs-unstable nixpkgs

# Setup the env
RUN mkdir -p vector/scripts
ADD default.nix shell.nix rust-toolchain .envrc /vector/
ADD scripts/Gemfile scripts/Gemfile.lock /vector/scripts/
WORKDIR /vector
SHELL [ "/usr/bin/env", "nix-shell", "/vector/shell.nix", "--run" ]
RUN echo "Installing env..."

# Setup the toolchain
RUN rustup default $(cat rust-toolchain)
WORKDIR /vector/scripts
RUN bundle update --bundler
RUN bundle install
WORKDIR /vector/website
RUN yarn
WORKDIR /vector

# Declare volumes
VOLUME /vector
VOLUME /vector/target
VOLUME /root/.cargo

# Prepare for use
ADD ./scripts/environment/entrypoint.sh /
ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "bash" ]
