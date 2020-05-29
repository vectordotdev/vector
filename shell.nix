scope@{ pkgs ? import <nixpkgs> {} }:

let env = (import ./default.nix scope); in

pkgs.mkShell {
  PROTOC="${pkgs.protobuf}/bin/protoc";
  PROTOC_INCLUDE="${pkgs.protobuf}/include";
  LOCALE_ARCHIVE="${pkgs.glibcLocales}/lib/locale/locale-archive";
  LC_ALL="en_US.UTF-8";
  buildInputs = [ (import ./default.nix { inherit pkgs; }) ];
  OPENSSL_DIR="${pkgs.openssl.dev}";
  OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib";
  LIBCLANG_PATH ="${pkgs.llvmPackages.libclang}/lib";
}
