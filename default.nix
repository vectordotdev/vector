{ pkgs ? import <nixpkgs> {} }:

pkgs.buildEnv {
  name = "vector-env";
  paths = with pkgs; [
      bash
      binutils
      gcc
      cmake
      rustup 
      pkg-config
      openssl
      protobuf
      rdkafka
      ruby.devEnv
      yarn
      openssl
      perl
      remarshal
      snappy
      gnumake
      autoconf
    ]  ++ stdenv.lib.optional stdenv.isDarwin [ Security libiconv ];
}