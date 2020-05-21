FROM nixos/nix
ARG RUST_TOOLCHAIN

RUN nix-channel --add https://nixos.org/channels/nixpkgs-unstable nixpkgs
RUN nix-channel --update

RUN mkdir -p vector/scripts
ADD shell.nix default.nix /vector/
ADD scripts/Gemfile scripts/Gemfile.lock /vector/scripts/
WORKDIR /vector
# Prebuild
RUN nix-env -i -f default.nix
RUN rustup default $RUST_TOOLCHAIN
WORKDIR /vector/scripts
RUN bundle install
WORKDIR /vector

VOLUME /vector
VOLUME /vector/target
VOLUME /root/.cargo
CMD nix-shell