args@{
  rustTarget ? null,
  linking ? "dynamic",
  cross ? null,
  pkgs ? (import <nixpkgs> {
    overlays = [
      (import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz))
    ];
  }),
  ...
}:

rec {
  environmentVariables =  {
    PKG_CONFIG_ALLOW_CROSS=true;
    # We must set some protoc related env vars for the prost crate.
    PROTOC = "${pkgs.protobuf}/bin/protoc";
    PROTOC_INCLUDE = "${pkgs.protobuf}/include";
    # On Linux builds, we need some level of localization.
    LOCALE_ARCHIVE = if pkgs.stdenv.isLinux && pkgs.glibcLocales != null then
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

  developmentTools = with pkgs; [
    # Core CLI tools
    dnsutils
    curl
    bash
    nix
    direnv
    binutils
    remarshal
    libiconv
    tzdata
    jq
    stdenv
    bashInteractive
    rustup
    # Build Env
    git
    cacert
    ruby_2_7
    nodejs
    yarn
    shellcheck
    # Container tools
    docker
    docker-compose
    # Wasm
    llvmPackages.libclang
  ]  ++ (if stdenv.isDarwin then [
    # Mac only
  ] else [
    # Linux
    podman
    podman-compose
  ]) ++ nativeBuildInputs ++ buildInputs;

  # From: https://nixos.org/nixpkgs/manual/
  #
  # A list of dependencies whose host platform is the new derivation's build platform, and target
  # platform is the new derivation's host platform. This means a -1 host offset and 0 target
  # offset from the new derivation's platforms. These are programs and libraries used at build-time
  # that, if they are a compiler or similar tool, produce code to run at run-time—i.e. tools used
  # to build the new derivation. If the dependency doesn't care about the target platform (i.e.
  # isn't a compiler or similar tool), put it here, rather than in depsBuildBuild or
  # depsBuildTarget. This could be called depsBuildHost but nativeBuildInputs is used for
  # historical continuity.
  #
  # Since these packages are able to be run at build-time, they are added to the PATH, as described
  # above. But since these packages are only guaranteed to be able to run then, they shouldn't
  # persist as run-time dependencies. This isn't currently enforced, but could be in the future.
  nativeBuildInputs = with (if args ? cross && args.cross != null then cross else pkgs); [
    pkg-config
    rdkafka
    openssl.dev
    jemalloc
  ] ++ (if stdenv.isDarwin then [
    darwin.cf-private
    darwin.apple_sdk.frameworks.CoreServices
    darwin.apple_sdk.frameworks.Security
    darwin.apple_sdk.frameworks.SecurityFoundation
  ] else [
    linuxHeaders
    musl
    libgcc
  ]); 
  # ++ (if pkgs.glibcLocales != null then [
  #   glibcLocales.override { locales = ["en_US.UTF-8"]; }
  # ] else []);

  # From: https://nixos.org/nixpkgs/manual/
  #
  # A list of dependencies whose host platform and target platform match the new derivation's.
  # This means a 0 host offset and a 1 target offset from the new derivation's host platform. This
  # would be called depsHostTarget but for historical continuity. If the dependency doesn't care
  # about the target platform (i.e. isn't a compiler or similar tool), put it here, rather than in
  # depsBuildBuild.
  #
  # These are often programs and libraries used by the new derivation at run-time, but that isn't
  # always the case. For example, the machine code in a statically-linked library is only used at
  # run-time, but the derivation containing the library is only needed at build-time. Even in the
  # dynamic case, the library may also be needed at build-time to appease the linker.
  buildInputs = with pkgs; [
    protobuf
    rustup
    rdkafka
  ] ++ (if stdenv.isDarwin then [
  ] else [
    systemd
  ]);
}
