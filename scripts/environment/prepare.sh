#! /usr/bin/env bash
set -e -o verbose

curl https://sh.rustup.rs -sSf | sh -s -- -y
source $HOME/.cargo/env

rustup target add wasm32-wasi
rustup toolchain install nightly --target x86_64-unknown-linux-musl
rustup toolchain install nightly --target armv7-unknown-linux-musleabihf
rustup toolchain install nightly --target aarch64-unknown-linux-musl
rustup component add rustfmt
rustup component add clippy
rustup default "$(cat rust-toolchain)"

cd scripts
# Ruby toolchain
export PATH="${HOME}/.rbenv/bin:${HOME}/.rbenv/shims:${PATH}"
curl -fsSL https://github.com/rbenv/rbenv-installer/raw/master/bin/rbenv-installer | bash
eval "$(rbenv init -)"
rbenv install 2.7.1
rbenv global 2.7.1
bundle update --bundler
bundle install
cd ..

cd scripts
# Node toolchain
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.35.3/install.sh | bash
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"  # This loads nvm
[ -s "$NVM_DIR/bash_completion" ] && \. "$NVM_DIR/bash_completion"  # This loads nvm bash_completion
nvm install 10.19.0
nvm use 10.19.0
nvm alias default 10.19.0
npm install markdownlint-cli
cd ..

pip3 install jsonschema==3.2.0
pip3 install remarshal==0.11.2
