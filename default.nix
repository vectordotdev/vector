{ pkgs ? import <nixpkgs> {} }:

pkgs.buildEnv {
  name = "vector-env";
  paths = with pkgs; [
      bash
      git
      binutils
      gcc
      cmake
      rustup 
      pkg-config
      openssl
      protobuf
      rdkafka
      bundler
      yarn
      openssl
      perl
      remarshal
      snappy
      gnumake
      autoconf
    ]  ++ stdenv.lib.optional stdenv.isDarwin [ Security libiconv ];
}