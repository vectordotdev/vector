#! /usr/bin/env bash
set -e -o verbose

# Rust
curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal
rustup default "$(cat rust-toolchain)"
rustup component add rustfmt
rustup component add clippy
rustup target add wasm32-wasi

cd scripts
# Node toolchain
export PATH="${HOME}/.rbenv/bin:${HOME}/.rbenv/shims:${PATH}"
curl -fsSL https://github.com/rbenv/rbenv-installer/raw/master/bin/rbenv-installer | bash
eval "$(rbenv init -)"
rbenv install 2.7.1
rbenv global 2.7.1

bundle update --bundler
bundle install
cd ..

cd website
# Node toolchain
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.35.3/install.sh | bash
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"  # This loads nvm
[ -s "$NVM_DIR/bash_completion" ] && \. "$NVM_DIR/bash_completion"  # This loads nvm bash_completion
nvm install 10.19.0
nvm use 10.19.0
nvm alias default 10.19.0
# Yarn
export PATH="$HOME/.yarn/bin:$HOME/.config/yarn/global/node_modules/.bin:$PATH"
curl -o- -L https://yarnpkg.com/install.sh | bash
yarn install
cd ..
