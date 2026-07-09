#!/usr/bin/env bash
set -euo pipefail

ensure_active_toolchain_is_installed() {
  if ! command -v rustup >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  fi

  # Ensure cargo/rustup are on PATH even if rustup was preinstalled in the image
  if [ -f "${HOME}/.cargo/env" ]; then
    # shellcheck source=/dev/null
    source "${HOME}/.cargo/env"
  fi

  # Determine desired toolchain and ensure it's installed.
  ACTIVE_TOOLCHAIN="$(rustup show active-toolchain 2>/dev/null || true)"
  ACTIVE_TOOLCHAIN="${ACTIVE_TOOLCHAIN%% *}"  # keep only the first token
  if [ -z "${ACTIVE_TOOLCHAIN}" ]; then
    # No active toolchain yet: fall back to env override or ultimately to stable.
    ACTIVE_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
    rustup default "${ACTIVE_TOOLCHAIN}"
  fi

  rustup toolchain install "${ACTIVE_TOOLCHAIN}"
  rustup show
}

SCRIPT_DIR=$(realpath "$(dirname "${BASH_SOURCE[0]}")")

# Tool version definitions - update versions here
CARGO_DEB_VERSION="2.9.3"
CROSS_VERSION="0.2.5"
CARGO_NEXTEST_VERSION="0.9.95"
CARGO_DENY_VERSION="0.19.0"
CARGO_MSRV_VERSION="0.18.4"
CARGO_HACK_VERSION="0.6.43"
DD_RUST_LICENSE_TOOL_VERSION="1.0.6"
CARGO_LLVM_COV_VERSION="0.8.4"
WASM_PACK_VERSION="0.15.0"
# npm tool versions are defined in scripts/environment/npm-tools/package.json
# and pinned (including transitive deps) in npm-tools/package-lock.json.

ALL_MODULES=(
  rustup
  cargo-deb
  cross
  cargo-nextest
  cargo-deny
  cargo-msrv
  cargo-hack
  cargo-llvm-cov
  dd-rust-license-tool
  wasm-pack
  markdownlint-cli2
  prettier
  datadog-ci
  vdev
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
  cargo-msrv
  cargo-hack
  cargo-llvm-cov
  dd-rust-license-tool
  wasm-pack
  markdownlint-cli2
  prettier
  datadog-ci
  vdev

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

# Helper function to check version and install if needed
# Usage: maybe_install_cargo_tool <tool-name> [<version> [<version-check-pattern>]]
# Note: cargo-* tools are invoked as "cargo <subcommand>", not as direct binaries
# vdev omits the version argument: binstall reads it from vdev/Cargo.toml via
# --manifest-path, which also provides the pkg-url so crates.io is not consulted.
maybe_install_cargo_tool() {
  local tool="$1"
  local version="${2:-}"
  local version_pattern="${3:-${tool} ${version}}"  # Default to "tool version"

  if ! contains_module "$tool"; then
    return 0
  fi

  # For cargo-* tools, invoke as "cargo <subcommand>" not "cargo-<subcommand>"
  local version_cmd="$tool"
  if [[ "$tool" == cargo-* ]]; then
    version_cmd="cargo ${tool#cargo-}"
  fi

  # vdev: binstall reads the version and pkg-url from vdev/Cargo.toml via
  # --manifest-path, so vdev/Cargo.toml is the single source of truth.
  # `--disable-strategies compile` skips binstall's own crates.io compile
  # fallback (which would fail anyway on an unpublished version, and
  # silently slow-paths a cache flake into a multi-minute build). If
  # binstall fails for any reason — missing prebuilt for a freshly bumped
  # version, or a transient cache/network issue — we fall back to building
  # from the working tree. That gives PRs that bump vdev's version a
  # working binary built from their own changes, and keeps master CI green
  # in the gap between merging a vdev version bump and pushing the tag.
  if [[ "$tool" == "vdev" ]]; then
    # Skip the install entirely when the cached binary already matches the
    # version in vdev/Cargo.toml (the setup workflow restores ~/.cargo/bin/vdev
    # from cache, but not binstall's resolution metadata, so without this check
    # every job with `vdev: true` would re-hit GitHub releases).
    local cargo_toml_version
    cargo_toml_version=$(grep -E '^version = ' vdev/Cargo.toml | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
    if vdev --version 2>/dev/null | grep -q "^vdev ${cargo_toml_version}$"; then
      echo "vdev ${cargo_toml_version} already installed"
      return 0
    fi

    local installer=("${install[@]}")
    if [[ "${installer[0]}" == "binstall" ]]; then
      installer+=(--disable-strategies compile)
      if ! cargo "${installer[@]}" --manifest-path vdev/Cargo.toml vdev; then
        echo "binstall failed; building vdev from the working tree..."
        cargo install -f --path vdev --locked
      fi
    else
      # binstall unavailable. `cargo install vdev` (no version) would resolve
      # against crates.io and could pick an older version than the checkout
      # declares; install from the working tree to match vdev/Cargo.toml.
      cargo install -f --path vdev --locked
    fi
    return 0
  fi

  if ! $version_cmd --version 2>/dev/null | grep -q "^${version_pattern}"; then
    local should_install=true
    # Outside CI, preserve a newer-than-pin version the user already has.
    # `cargo install --force` would otherwise silently downgrade them.
    if [[ -z "${CI:-}" ]]; then
      local current
      current=$($version_cmd --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)
      if [[ -n "$current" ]] && [[ "$current" != "$version" ]]; then
        local newest
        newest=$(printf '%s\n%s\n' "$current" "$version" | sort -V | tail -1)
        if [[ "$newest" == "$current" ]]; then
          echo "Keeping ${tool} ${current} (newer than pin ${version}). Set CI=1 to force the pin."
          should_install=false
        fi
      fi
    fi
    if [[ "$should_install" == "true" ]]; then
      cargo "${install[@]}" "$tool" --version "$version" --force --locked
    fi
  fi

  # cargo-llvm-cov requires the llvm-tools-preview rustup component
  if [[ "$tool" == "cargo-llvm-cov" ]]; then
    rustup component add llvm-tools-preview
  fi
}

# Install npm tools from the committed package-lock.json so that every
# transitive dependency version is pinned (no live registry resolution).
# Versions are defined in npm-tools/package.json; npm ci ensures exact lockfile match.
# Note: npm ci installs all packages in the lockfile even if only one tool
# is requested, since it does not support selective installation.
maybe_install_npm_tools() {
  local npm_tools=(markdownlint-cli2 prettier datadog-ci)

  # Early return when no npm tool is requested, so hosts without npm
  # (e.g. tests/e2e/Dockerfile calling prepare.sh --modules=cargo-nextest)
  # are not broken by the npm commands below.
  local any_requested=false
  for tool in "${npm_tools[@]}"; do
    if contains_module "$tool"; then
      any_requested=true
      break
    fi
  done
  if [[ "$any_requested" == "false" ]]; then
    return 0
  fi

  local npm_tools_dir="${SCRIPT_DIR}/npm-tools"
  local npm_bin_dir
  npm_bin_dir="$(npm config get prefix -g)/bin"
  local need_install=false

  for tool in "${npm_tools[@]}"; do
    if contains_module "$tool"; then
      local expected="${npm_tools_dir}/node_modules/.bin/${tool}"
      if [[ "$(readlink "${npm_bin_dir}/${tool}" 2>/dev/null)" != "$expected" ]] || [[ ! -x "$expected" ]]; then
        need_install=true
        break
      fi
    fi
  done

  if [[ "$need_install" == "false" ]]; then
    return 0
  fi

  npm ci --prefix "${npm_tools_dir}"

  # Outside CI, skip the global symlink to avoid a sudo write to /usr/local/bin
  # (or equivalent). The Makefile prepends this directory to PATH, so `make`
  # recipes find the tools automatically.
  if [[ -z "${CI:-}" ]]; then
    echo "npm tools installed under ${npm_tools_dir}/node_modules/.bin"
    echo "Make recipes discover them automatically. To invoke directly from a"
    echo "shell, add the directory to your PATH:"
    echo "  export PATH=\"${npm_tools_dir}/node_modules/.bin:\$PATH\""
    return 0
  fi

  # Use sudo only when the target directory is not writable (e.g. /usr/local/bin
  # on Linux CI runners is root-owned, but Homebrew dirs on macOS are user-owned).
  local ln_cmd=(ln -sf)
  if [[ ! -w "${npm_bin_dir}" ]]; then
    ln_cmd=(sudo ln -sf)
  fi
  for tool in "${npm_tools[@]}"; do
    "${ln_cmd[@]}" "${npm_tools_dir}/node_modules/.bin/${tool}" "${npm_bin_dir}/${tool}"
  done
}

# Set git safe.directory in CI where the repo may be checked out by a different
# uid than the user running git. Skipped on workstations: the contributor owns
# the checkout and a global config write is unnecessary.
if [[ -n "${CI:-}" ]]; then
  git config --global --add safe.directory "$(pwd)"
fi

REQUIRES_RUSTUP=(dd-rust-license-tool cargo-deb cross cargo-nextest cargo-deny cargo-msrv cargo-hack cargo-llvm-cov wasm-pack vdev)
REQUIRES_BINSTALL=(cargo-deb cross cargo-nextest cargo-deny cargo-msrv cargo-hack cargo-llvm-cov wasm-pack vdev)
require_binstall=false

for tool in "${REQUIRES_BINSTALL[@]}"; do
  if contains_module "$tool"; then
    require_binstall=true
    MODULES=(rustup "${MODULES[@]}")
    break
  fi
done

if [ "${require_binstall}" = "false" ] && ! contains_module rustup; then
  for tool in "${REQUIRES_RUSTUP[@]}"; do
    if contains_module "$tool"; then
      MODULES=(rustup "${MODULES[@]}")
      break
    fi
  done
fi

install=(install)
if contains_module rustup; then
  ensure_active_toolchain_is_installed

  if [ "${require_binstall}" = "true" ]; then
    if cargo binstall -V &>/dev/null || "${SCRIPT_DIR}"/binstall.sh; then
      install=(binstall -y)
    else
      echo "Failed to install cargo binstall, defaulting to cargo install"
    fi
  fi
fi
set -e -o verbose

maybe_install_cargo_tool cargo-deb "${CARGO_DEB_VERSION}" "${CARGO_DEB_VERSION}"
maybe_install_cargo_tool cross "${CROSS_VERSION}"
maybe_install_cargo_tool cargo-nextest "${CARGO_NEXTEST_VERSION}"
maybe_install_cargo_tool cargo-deny "${CARGO_DENY_VERSION}"
maybe_install_cargo_tool cargo-msrv "${CARGO_MSRV_VERSION}"
maybe_install_cargo_tool cargo-hack "${CARGO_HACK_VERSION}"
maybe_install_cargo_tool cargo-llvm-cov "${CARGO_LLVM_COV_VERSION}"
maybe_install_cargo_tool dd-rust-license-tool "${DD_RUST_LICENSE_TOOL_VERSION}"
maybe_install_cargo_tool wasm-pack "${WASM_PACK_VERSION}"
maybe_install_cargo_tool vdev

maybe_install_npm_tools
