#!/usr/bin/env bash
set -euo pipefail

ALL_MODULES=(
  rustup
  cargo-deb
  cross
  cargo-nextest
  cargo-deny
  dd-rust-license-tool
  wasm-pack
  markdownlint
  datadog-ci
  release-flags
)

# By default, install everything
MODULES=( "${ALL_MODULES[@]}" )

# Helper to join an array by comma
join_by() { local IFS="$1"; shift; echo "$*"; }

# If the INSTALL_MODULES env var is set, override MODULES
if [[ -n "${INSTALL_MODULES:-}" ]]; then
  IFS=',' read -r -a MODULES <<< "$INSTALL_MODULES"
fi

# Parse CLI args for --modules or -m
for arg in "$@"; do
  case $arg in
    --modules=*|-m=*)
      val="${arg#*=}"
      IFS=',' read -r -a MODULES <<< "$val"
      shift
      ;;
    --help|-h)
      cat <<EOF
Usage: $0 [--modules=mod1,mod2,...]

Modules:
  rustup
  cargo-deb
  cross
  cargo-nextest
  cargo-deny
  dd-rust-license-tool
  wasm-pack
  markdownlint
  datadog-ci

If a module requires rust then rustup will be automatically installed.
By default, all modules are installed. To install only a subset:
  INSTALL_MODULES=cargo-deb,cross    # via env var
  $0 --modules=cargo-deb,cross       # via CLI
EOF
      exit 0
      ;;
    *)
      echo "Unknown option: $arg"
      exit 1
      ;;
  esac
done

echo "Installing modules: $(join_by ', ' "${MODULES[@]}")"

contains_module() {
  local needle="$1"
  for item in "${MODULES[@]}"; do
    [[ "$item" == "$needle" ]] && return 0
  done
  return 1
}

# Always ensure git safe.directory is set
git config --global --add safe.directory "$(pwd)"

if ! contains_module rustup; then
  REQUIRES_RUSTUP=(cargo-deb cross cargo-nextest cargo-deny dd-rust-license-tool wasm-pack)
  for tool in "${REQUIRES_RUSTUP[@]}"; do
    if contains_module "$tool"; then
      MODULES=(rustup "${MODULES[@]}")
      break
    fi
  done
fi

if contains_module rustup; then
  . scripts/environment/release-flags.sh

  rustup show active-toolchain || rustup toolchain install stable
  rustup show
fi

if contains_module cargo-deb; then
  if [[ "$(cargo-deb --version 2>/dev/null)" != "2.0.2" ]]; then
    rustup run stable cargo install cargo-deb --version 2.0.2 --force --locked
  fi
fi

if contains_module cross; then
  if ! cross --version 2>/dev/null | grep -q '^cross 0.2.5'; then
    rustup run stable cargo install cross --version 0.2.5 --force --locked
  fi
fi

if contains_module cargo-nextest; then
  if ! cargo-nextest --version 2>/dev/null | grep -q '^cargo-nextest 0.9.95'; then
    rustup run stable cargo install cargo-nextest --version 0.9.95 --force --locked
  fi
fi

if contains_module cargo-deny; then
  if ! cargo-deny --version 2>/dev/null | grep -q '^cargo-deny 0.16.2'; then
    rustup run stable cargo install cargo-deny --version 0.16.2 --force --locked
  fi
fi

if contains_module dd-rust-license-tool; then
  if ! dd-rust-license-tool --help &>/dev/null; then
    rustup run stable cargo install dd-rust-license-tool --version 1.0.2 --force --locked
  fi
fi

if contains_module wasm-pack; then
  if ! wasm-pack --version | grep -q '^wasm-pack 0.13.1'; then
    rustup run stable cargo install --locked --version 0.13.1 wasm-pack
  fi
fi

if contains_module markdownlint; then
  if [[ "$(markdownlint --version 2>/dev/null)" != "0.45.0" ]]; then
    sudo npm install -g markdownlint-cli@0.45.0
  fi
fi

if contains_module datadog-ci; then
  if [[ "$(datadog-ci version 2>/dev/null)" != "v3.16.0" ]]; then
    sudo npm install -g @datadog/datadog-ci@3.16.0
  fi
fi
