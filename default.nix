scope@{ pkgs ? import <nixpkgs> {} }:

pkgs.buildEnv {
  name = "vector-env";
  paths = with pkgs; [
      git
      bash
      direnv
      binutils
      cmake
      rustup
      gcc
      openssl
      pkg-config
      protobuf
      rdkafka
      bundler
      docker
      (glibcLocales.override {
        locales = ["en_US.UTF-8"];
      })
      yarn
      openssl
      perl
      remarshal
      snappy
      gnumake
      autoconf
      (import (builtins.fetchGit {
        name = "wabt";
        url = "https://github.com/nixos/nixpkgs-channels/";
        ref = "refs/heads/nixpkgs-unstable";
        rev = "f61b3e02c05d36c58cb5f5fc793c38df5a79e490";
      }) {}).wabt
      llvmPackages.libclang
    ]  ++ stdenv.lib.optional stdenv.isDarwin [ Security libiconv ];
    passthru = {
        shellHook = ''
            export PROTOC="${pkgs.protobuf}/bin/protoc";
            export PROTOC_INCLUDE="${pkgs.protobuf}/include";
            export LOCALE_ARCHIVE="${pkgs.glibcLocales}/lib/locale/locale-archive";
            export LC_ALL="en_US.UTF-8";
            export OPENSSL_DIR="${pkgs.openssl.dev}";
            export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib";
            export LIBCLANG_PATH ="${pkgs.llvmPackages.libclang}/lib";
        '';
        PROTOC="${pkgs.protobuf}/bin/protoc";
        PROTOC_INCLUDE="${pkgs.protobuf}/include";
        LOCALE_ARCHIVE="${pkgs.glibcLocales}/lib/locale/locale-archive";
        LC_ALL="en_US.UTF-8";
        OPENSSL_DIR="${pkgs.openssl.dev}";
        OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib";
        LIBCLANG_PATH ="${pkgs.llvmPackages.libclang}/lib";
    };
}
