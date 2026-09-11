#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(realpath "$(dirname "${BASH_SOURCE[0]}")")

# Tool versions. Keep pins here so CI and local setup use the same versions.
CARGO_DEB_VERSION="2.9.3"
CROSS_VERSION="0.2.5"
CARGO_NEXTEST_VERSION="0.9.95"
CARGO_DENY_VERSION="0.19.0"
CARGO_MSRV_VERSION="0.18.4"
CARGO_HACK_VERSION="0.6.43"
DD_RUST_LICENSE_TOOL_VERSION="1.0.6"
CARGO_LLVM_COV_VERSION="0.8.4"
WASM_PACK_VERSION="0.15.0"
CUE_VERSION="0.17.1"
MOLD_VERSION="2.40.4"

# The no-argument default remains workstation-safe. CI requests system-level
# dependencies explicitly through .github/actions/setup/action.yml.
DEFAULT_MODULES=(
  rustup
  protoc
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

SYSTEM_MODULES=(
  libsasl2
  unixodbc
  cmark-gfm
  cross-binutils
  rpm
  lcov
  bc
)

SUPPORTED_MODULES=(
  "${DEFAULT_MODULES[@]}"
  cue
  mold
  "${SYSTEM_MODULES[@]}"
)

MODULES=("${DEFAULT_MODULES[@]}")

# General helpers

join_by() {
  local IFS="$1"
  shift
  echo "$*"
}

contains_module() {
  local needle="$1"
  local item
  for item in "${MODULES[@]}"; do
    [[ "$item" == "$needle" ]] && return 0
  done
  return 1
}

is_supported_module() {
  local needle="$1"
  local item
  for item in "${SUPPORTED_MODULES[@]}"; do
    [[ "$item" == "$needle" ]] && return 0
  done
  return 1
}

print_usage() {
  cat <<EOF
Usage: $0 [--modules=mod1,mod2,...]

Supported modules:
EOF
  printf '  %s\n' "${SUPPORTED_MODULES[@]}"
  cat <<EOF

If a module requires Rust, rustup is installed automatically.
By default, developer tooling is installed without system packages. To install
only a subset:
  INSTALL_MODULES=cargo-deb,cross    # via environment variable
  $0 --modules=cargo-deb,cross       # via CLI
EOF
}

parse_args() {
  if [[ -n "${INSTALL_MODULES:-}" ]]; then
    IFS=',' read -r -a MODULES <<<"$INSTALL_MODULES"
  fi

  local arg
  for arg in "$@"; do
    case "$arg" in
      --modules=*|-m=*)
        IFS=',' read -r -a MODULES <<<"${arg#*=}"
        ;;
      --help|-h)
        print_usage
        exit 0
        ;;
      *)
        echo "Unknown option: $arg" >&2
        exit 1
        ;;
    esac
  done

  local module
  for module in "${MODULES[@]}"; do
    if ! is_supported_module "$module"; then
      echo "Unknown module: $module" >&2
      exit 1
    fi
  done
}

# Rust and Cargo tooling

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
  local active_toolchain
  active_toolchain="$(rustup show active-toolchain 2>/dev/null || true)"
  active_toolchain="${active_toolchain%% *}" # keep only the first token
  if [[ -z "$active_toolchain" ]]; then
    # No active toolchain yet: fall back to env override or ultimately to stable.
    active_toolchain="${RUSTUP_TOOLCHAIN:-stable}"
    rustup default "$active_toolchain"
  fi

  rustup toolchain install "$active_toolchain"
  rustup show
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
  local version_cmd=("$tool")
  if [[ "$tool" == cargo-* ]]; then
    version_cmd=(cargo "${tool#cargo-}")
  fi

  # vdev: binstall reads the version and pkg-url from vdev/Cargo.toml via
  # --manifest-path, so vdev/Cargo.toml is the single source of truth.
  # `--disable-strategies compile` skips binstall's own crates.io compile
  # fallback (which would fail anyway on an unpublished version, and
  # silently slow-paths a cache flake into a multi-minute build). We always
  # force the install because the version alone cannot identify the source
  # code of an unmerged checkout. If no prebuilt binary exists, build the
  # current checkout instead.
  if [[ "$tool" == "vdev" ]]; then
    local vdev_installer=("${cargo_tool_installer[@]}")
    if [[ "${vdev_installer[0]}" == "binstall" ]]; then
      vdev_installer+=(--force --disable-strategies compile)
      if ! cargo "${vdev_installer[@]}" --manifest-path vdev/Cargo.toml vdev; then
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

  if ! "${version_cmd[@]}" --version 2>/dev/null | grep -q "^${version_pattern}"; then
    local should_install=true
    # Outside CI, preserve a newer-than-pin version the user already has.
    # `cargo install --force` would otherwise silently downgrade them.
    if [[ "${CI:-}" != "true" ]]; then
      local current
      current=$("${version_cmd[@]}" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)
      if [[ -n "$current" ]] && [[ "$current" != "$version" ]]; then
        local newest
        newest=$(printf '%s\n%s\n' "$current" "$version" | sort -V | tail -1)
        if [[ "$newest" == "$current" ]]; then
          echo "Keeping ${tool} ${current} (newer than pin ${version}). Set CI=true to force the pin."
          should_install=false
        fi
      fi
    fi
    if [[ "$should_install" == "true" ]]; then
      cargo "${cargo_tool_installer[@]}" "$tool" --version "$version" --force --locked
    fi
  fi

  # cargo-llvm-cov requires the llvm-tools-preview rustup component
  if [[ "$tool" == "cargo-llvm-cov" ]]; then
    rustup component add llvm-tools-preview
  fi
}

# Standalone binaries

install_bin_dir() {
  if [[ "${CI:-}" == "true" ]]; then
    echo "${RUNNER_TEMP:?RUNNER_TEMP must be set when CI is enabled}/vector-tools-bin"
  else
    echo "${HOME}/.local/bin"
  fi
}

add_to_path() {
  local dir="$1"
  export PATH="${dir}:${PATH}"
  if [[ -n "${GITHUB_PATH:-}" ]]; then
    echo "$dir" >>"${GITHUB_PATH}"
  fi
}

install_cue() {
  contains_module cue || return 0

  local current_version
  current_version=$(cue version 2>/dev/null | sed -n '1p' || true)
  if [[ "$current_version" == "cue version v${CUE_VERSION}" ]]; then
    return 0
  fi

  local os arch
  case "$(uname -s)" in
    Linux) os=linux ;;
    Darwin) os=darwin ;;
    *)
      echo "The cue module does not support this platform." >&2
      return 1
      ;;
  esac
  case "$(uname -m)" in
    x86_64) arch=amd64 ;;
    arm64 | aarch64) arch=arm64 ;;
    *)
      echo "The cue module does not support architecture $(uname -m)." >&2
      return 1
      ;;
  esac

  local archive="cue_v${CUE_VERSION}_${os}_${arch}.tar.gz"
  local temp install_dir
  temp=$(mktemp -d)
  install_dir=$(install_bin_dir)
  mkdir -p "$install_dir"

  curl -fsSL "https://github.com/cue-lang/cue/releases/download/v${CUE_VERSION}/${archive}" \
    --output "${temp}/${archive}"
  tar -xzf "${temp}/${archive}" -C "$temp" cue
  install -m 0755 "${temp}/cue" "${install_dir}/cue"
  rm -rf "$temp"
  add_to_path "$install_dir"
}

install_mold() {
  contains_module mold || return 0

  if [[ "$(uname -s)" != "Linux" ]]; then
    echo "The mold module is only supported on Linux." >&2
    return 1
  fi
  if mold --version 2>/dev/null | grep -q "mold ${MOLD_VERSION}"; then
    return 0
  fi

  local machine target
  machine=$(uname -m)
  target="mold-${MOLD_VERSION}-${machine}-linux"
  local archive="${target}.tar.gz"
  local temp install_dir
  temp=$(mktemp -d)
  install_dir=$(install_bin_dir)
  mkdir -p "$install_dir"

  curl -fsSL "https://github.com/rui314/mold/releases/download/v${MOLD_VERSION}/${archive}" \
    --output "${temp}/${archive}"
  tar -xzf "${temp}/${archive}" -C "$temp"
  install -m 0755 "${temp}/${target}/bin/mold" "${install_dir}/mold"
  install -m 0755 "${temp}/${target}/lib/mold/mold-wrapper.so" "${install_dir}/mold-wrapper.so"
  rm -rf "$temp"
  add_to_path "$install_dir"
}

# System packages

run_privileged_with_timeout() {
  local cmd=("$@")
  if [[ "$(id -u)" -ne 0 ]]; then
    cmd=(sudo "${cmd[@]}")
  fi

  if command -v timeout >/dev/null 2>&1; then
    timeout 30m "${cmd[@]}"
  else
    "${cmd[@]}"
  fi
}

install_system_packages() {
  local os
  os=$(uname -s)

  case "$os" in
    Linux)
      local packages=()
      contains_module libsasl2 && packages+=(libsasl2-dev)
      contains_module unixodbc && packages+=(unixodbc-dev)
      contains_module cmark-gfm && packages+=(cmark-gfm)
      contains_module cross-binutils && packages+=(binutils-arm-linux-gnueabihf binutils-aarch64-linux-gnu)
      contains_module rpm && packages+=(rpm)
      contains_module lcov && packages+=(lcov)
      contains_module bc && packages+=(bc)

      if [[ "${#packages[@]}" -eq 0 ]]; then
        return 0
      fi
      if ! command -v apt-get >/dev/null 2>&1; then
        echo "Requested system modules require apt-get on Linux." >&2
        return 1
      fi

      run_privileged_with_timeout apt-get update
      run_privileged_with_timeout apt-get install -y --no-install-recommends "${packages[@]}"
      ;;
    Darwin)
      local packages=()
      contains_module libsasl2 && packages+=(cyrus-sasl)
      contains_module unixodbc && packages+=(unixodbc)
      contains_module cmark-gfm && packages+=(cmark-gfm)
      contains_module rpm && packages+=(rpm)
      contains_module lcov && packages+=(lcov)
      contains_module bc && packages+=(bc)

      if contains_module cross-binutils; then
        echo "The cross-binutils module is only supported on Linux." >&2
        return 1
      fi
      if [[ "${#packages[@]}" -gt 0 ]]; then
        brew install "${packages[@]}"
      fi
      if contains_module unixodbc; then
        # odbc-sys also probes `brew --prefix`, but these keep the linker,
        # headers, and pkg-config consistent for Intel and Apple Silicon.
        local prefix lib_dir include_dir pkgconfig_dir
        prefix="$(brew --prefix unixodbc)"
        lib_dir="${prefix}/lib"
        include_dir="${prefix}/include"
        pkgconfig_dir="${prefix}/lib/pkgconfig"
        export LIBRARY_PATH="${lib_dir}${LIBRARY_PATH:+:$LIBRARY_PATH}"
        export CPATH="${include_dir}${CPATH:+:$CPATH}"
        export PKG_CONFIG_PATH="${pkgconfig_dir}${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
        export DYLD_FALLBACK_LIBRARY_PATH="${lib_dir}${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
        if [[ -n "${GITHUB_ENV:-}" ]]; then
          {
            echo "LIBRARY_PATH=${LIBRARY_PATH}"
            echo "CPATH=${CPATH}"
            echo "PKG_CONFIG_PATH=${PKG_CONFIG_PATH}"
            echo "DYLD_FALLBACK_LIBRARY_PATH=${DYLD_FALLBACK_LIBRARY_PATH}"
          } >>"${GITHUB_ENV}"
        fi
      fi
      ;;
    *)
      local module
      for module in "${SYSTEM_MODULES[@]}"; do
        if contains_module "$module"; then
          echo "System module $module does not support this platform." >&2
          return 1
        fi
      done
      ;;
  esac
}

# npm tooling

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
  if [[ "${CI:-}" != "true" ]]; then
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

# Installation orchestration

REQUIRES_RUSTUP=(dd-rust-license-tool cargo-deb cross cargo-nextest cargo-deny cargo-msrv cargo-hack cargo-llvm-cov wasm-pack vdev)
REQUIRES_BINSTALL=(cargo-deb cross cargo-nextest cargo-deny cargo-msrv cargo-hack cargo-llvm-cov wasm-pack vdev)
require_binstall=false
cargo_tool_installer=(install)

resolve_rust_dependencies() {
  local tool
  for tool in "${REQUIRES_BINSTALL[@]}"; do
    if contains_module "$tool"; then
      require_binstall=true
      break
    fi
  done

  if contains_module rustup; then
    return 0
  fi
  for tool in "${REQUIRES_RUSTUP[@]}"; do
    if contains_module "$tool"; then
      MODULES=(rustup "${MODULES[@]}")
      return 0
    fi
  done
}

prepare_rust_installer() {
  contains_module rustup || return 0

  ensure_active_toolchain_is_installed
  if [[ "$require_binstall" == "true" ]]; then
    if cargo binstall -V &>/dev/null || "${SCRIPT_DIR}"/binstall.sh; then
      cargo_tool_installer=(binstall -y)
    else
      echo "Failed to install cargo binstall, defaulting to cargo install"
    fi
  fi
}

install_protoc() {
  contains_module protoc || return 0

  local protoc_dir
  if [[ "${CI:-}" == "true" ]]; then
    protoc_dir="${RUNNER_TEMP:?RUNNER_TEMP must be set when CI is enabled}/protoc-bin"
  else
    protoc_dir="${HOME}/.local/bin"
  fi

  bash "${SCRIPT_DIR}/install-protoc.sh" "${protoc_dir}"
  export PATH="${protoc_dir}:${PATH}"

  if [[ -n "${GITHUB_PATH:-}" ]]; then
    echo "${protoc_dir}" >> "${GITHUB_PATH}"
  fi
}

install_cargo_tools() {
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
}

main() {
  parse_args "$@"
  resolve_rust_dependencies
  echo "Installing modules: $(join_by ', ' "${MODULES[@]}")"

  # The checkout may be owned by another uid in CI containers.
  if [[ "${CI:-}" == "true" ]]; then
    git config --global --add safe.directory "$(pwd)"
  fi

  install_system_packages
  install_mold
  install_cue
  prepare_rust_installer
  install_protoc
  install_cargo_tools
  maybe_install_npm_tools
}

main "$@"
