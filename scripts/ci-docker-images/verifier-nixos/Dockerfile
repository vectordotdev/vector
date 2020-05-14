FROM nixos/nix:latest

RUN nix-env -i git ruby moreutils

RUN gem install bundler -v '~> 2.1.4'
COPY scripts/ci-docker-images/verifier-nixos/Gemfile Gemfile
RUN bundle install
RUN rm Gemfile

# Configure temp dir
ENV TMPDIR=/docker-tmp
RUN mkdir -p "$TMPDIR" && chmod 0777 "$TMPDIR"
VOLUME [ "$TMPDIR" ]
