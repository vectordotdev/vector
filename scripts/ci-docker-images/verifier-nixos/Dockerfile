FROM nixos/nix:latest

RUN nix-env -i git ruby

COPY Gemfile Gemfile
COPY Gemfile.lock Gemfile.lock
RUN bundle install
RUN rm Gemfile
