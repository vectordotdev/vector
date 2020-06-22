scope@{ pkgs ? import <nixpkgs> {} }:

let
  definition = (import ./scripts/environment/definition.nix scope);
in

pkgs.stdenv.mkDerivation ({
  name = "vector-shell";
  buildInputs = definition.packages;
} // definition.environmentVariables)
