FROM nixos/nix:latest

RUN nix-env -i git ruby

COPY ci-docker-images/verifier-nixos/Gemfile Gemfile
RUN bundle install
RUN rm Gemfile
