scope@{ pkgs ? import <nixpkgs> {} }:

{
  environmentVariables =  {
    PROTOC = "${pkgs.protobuf}/bin/protoc";
    PROTOC_INCLUDE = "${pkgs.protobuf}/include";
    LOCALE_ARCHIVE= if pkgs.stdenv.isLinux then
    "${pkgs.glibcLocales}/lib/locale/locale-archive"
    else
    "";
    LC_ALL = "en_US.UTF-8";
    OPENSSL_DIR = "${pkgs.openssl.dev}";
    OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
    SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
    GIT_SSL_CAINFO = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
    CURL_CA_BUNDLE = "${pkgs.cacert}/etc/ca-bundle.crt";
    CARGO_TERM_COLOR = "always";
    AWS_ACCESS_KEY_ID = "dummy";
    AWS_SECRET_ACCESS_KEY = "dummy";
    RUST_BACKTRACE = "full";
    TZDIR = "${pkgs.tzdata}/share/zoneinfo";
  };

  packages = with pkgs; [
    git
    dnsutils
    curl
    bash
    nix
    direnv
    binutils
    stdenv
    bashInteractive
    docker
    docker-compose
    cacert
    cmake
    rustup
    pkg-config
    openssl
    protobuf
    rdkafka
    ruby_2_7
    shellcheck
    docker
    yarn
    nodejs
    openssl
    perl
    remarshal
    snappy
    gnumake
    autoconf
    jq
    libiconv
    tzdata
  ] ++ (if stdenv.isDarwin then [
    darwin.cf-private
    darwin.apple_sdk.frameworks.CoreServices
    darwin.apple_sdk.frameworks.Security
    darwin.apple_sdk.frameworks.SecurityFoundation
  ] else [
    systemd gcc (glibcLocales.override { locales = ["en_US.UTF-8"]; })
  ]);
}
