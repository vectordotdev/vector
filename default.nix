scope@{ pkgs ? import <nixpkgs> {} }:

let definition = (import ./scripts/environment/definition.nix scope); in

pkgs.buildEnv {
  name = "vector-env";
  paths = definition.packages;
  passthru = definition.environmentVariables;
}
