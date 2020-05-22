scope@{ pkgs ? import <nixpkgs> {} }:

pkgs.buildEnv {
  name = "vector-env";
  paths = with pkgs; [
      git
      bash
      direnv
      binutils
      gcc
      cmake
      rustup 
      pkg-config
      openssl
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
    ]  ++ stdenv.lib.optional stdenv.isDarwin [ Security libiconv ];
    passthru = {
        shellHook = ''
            export PROTOC="${pkgs.protobuf}/bin/protoc";
            export PROTOC_INCLUDE="${pkgs.protobuf}/include";
            export LOCALE_ARCHIVE="${pkgs.glibcLocales}/lib/locale/locale-archive";
            export LC_ALL="en_US.UTF-8";
        '';
        PROTOC="${pkgs.protobuf}/bin/protoc";
        PROTOC_INCLUDE="${pkgs.protobuf}/include";
        LOCALE_ARCHIVE="${pkgs.glibcLocales}/lib/locale/locale-archive";
        LC_ALL="en_US.UTF-8";
    };
}