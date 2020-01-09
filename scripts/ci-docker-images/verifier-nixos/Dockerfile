FROM nixos/nix:latest

RUN nix-env -i git ruby

COPY verifier-nixos/Gemfile Gemfile
RUN bundle install
RUN rm Gemfile
