FROM nixos/nix
ARG RUST_TOOLCHAIN

RUN nix-channel --add https://nixos.org/channels/nixpkgs-unstable nixpkgs
RUN nix-channel --update

RUN mkdir -p vector/scripts
ADD default.nix shell.nix .envrc /vector/
ADD scripts/Gemfile scripts/Gemfile.lock /vector/scripts/
WORKDIR /vector
# Prebuild
RUN nix run -c rustup default $RUST_TOOLCHAIN
WORKDIR /vector/scripts
RUN nix run -f ../default.nix -c bundle update --bundler
RUN nix run -f ../default.nix -c bundle install
WORKDIR /vector/website
RUN nix run -f ../default.nix -c yarn
WORKDIR /vector

VOLUME /vector
VOLUME /vector/target
VOLUME /root/.cargo
RUN nix-env --install --file default.nix
ADD ./scripts/environment/entrypoint.sh /
ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "bash" ]
