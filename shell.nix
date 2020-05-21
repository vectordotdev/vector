{ nixpkgs ? import <nixpkgs> {} }:

with nixpkgs;

mkShell {
  PROTOC="${protobuf}/bin/protoc";
  PROTOC_INCLUDE="${protobuf}/include";
  buildInputs = [ (import ./default.nix { inherit pkgs; }) ];
}
