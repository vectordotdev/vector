FROM nixos/nix:latest

RUN nix-env -i git ruby

RUN gem install bundler -v '~> 2.1.4'
COPY ci-docker-images/verifier-nixos/Gemfile Gemfile
RUN bundle install
RUN rm Gemfile
