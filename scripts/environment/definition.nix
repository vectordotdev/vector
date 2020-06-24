scope@{ pkgs ? import <nixpkgs> {} }:

{
  environmentVariables =  {
    # We must set some protoc related env vars for the prost crate.
    PROTOC = "${pkgs.protobuf}/bin/protoc";
    PROTOC_INCLUDE = "${pkgs.protobuf}/include";
    # On Linux builds, we need some level of localization.
    LOCALE_ARCHIVE= if pkgs.stdenv.isLinux then
    "${pkgs.glibcLocales}/lib/locale/locale-archive"
    else
    "";
    LC_ALL = "en_US.UTF-8";
    # Without setting a tzdata folder, some tests will fail.
    TZDIR = "${pkgs.tzdata}/share/zoneinfo";
    # Crates expect information about OpenSSL in these vars.
    OPENSSL_DIR = "${pkgs.openssl.dev}";
    OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
    SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
    # Git looks to this env var for SSL certificates.
    GIT_SSL_CAINFO = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
    # Curl looks to this env var for SSL certificates.
    CURL_CA_BUNDLE = "${pkgs.cacert}/etc/ca-bundle.crt";
    # Encourage Cargo to be pretty.
    CARGO_TERM_COLOR = "always";
    # Enable backtraces in the environment.
    RUST_BACKTRACE = "full";
    # Vector gets very angry if you don't set these and use the AWS components.
    AWS_ACCESS_KEY_ID = "dummy";
    AWS_SECRET_ACCESS_KEY = "dummy";
    # Lucet (for wasm) depends on libclang
    LIBCLANG_PATH="${pkgs.llvmPackages.libclang}/lib";
    CPATH= if pkgs.stdenv.isLinux then
    "${pkgs.linuxHeaders}/include"
    else
    "";
  };

  packages = with pkgs; [
    # Core CLI tools
    bash
    bashInteractive
    binutils
    curl
    direnv
    dnsutils
    jq
    libiconv
    nix
    remarshal
    stdenv
    tzdata
    # Build Env
    autoconf
    cacert
    cmake
    cyrus_sasl
    git
    gnumake
    nodejs
    openssl
    perl
    pkg-config
    protobuf
    ruby_2_7
    rustup
    shellcheck
    snappy
    yarn
    # Container tools
    docker
    docker-compose
    # Wasm
    llvmPackages.libclang
  ] ++ (if stdenv.isDarwin then [
    darwin.apple_sdk.frameworks.CoreServices
    darwin.apple_sdk.frameworks.Security
    darwin.apple_sdk.frameworks.SecurityFoundation
    darwin.cf-private
  ] else [
    # Build
    gcc
    (glibcLocales.override { locales = ["en_US.UTF-8"]; })
    # Testing
    systemd
    # Container tools
    linuxHeaders
    podman
    podman-compose
  ]);
}
