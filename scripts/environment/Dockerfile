# Bootstrap
FROM docker.io/nixos/nix:2.3.4
RUN nix-channel --add https://nixos.org/channels/nixpkgs-unstable nixpkgs

# Setup the env
RUN mkdir -p vector/{scripts,website,scripts/environment}
ADD default.nix shell.nix rust-toolchain .envrc /vector/
ADD scripts/environment/definition.nix /vector/scripts/environment/
ADD scripts/Gemfile scripts/Gemfile.lock /vector/scripts/
ADD website/package.json website/yarn.lock /vector/website/
WORKDIR /vector
SHELL [ "/usr/bin/env", "nix-shell", "/vector/shell.nix", "--run" ]
RUN echo "Installing env..."

# Setup the toolchain
WORKDIR /vector
ADD ./scripts/environment/prepare.sh /
RUN ../prepare.sh

# Declare volumes
VOLUME /vector
VOLUME /vector/target
VOLUME /root/.cargo

# Prepare for use
ADD ./scripts/environment/entrypoint.sh /
ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "bash" ]
