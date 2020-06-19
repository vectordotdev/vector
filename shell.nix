scope@{ pkgs ? import <nixpkgs> {} }:

let
  definition = (import ./scripts/environment/definition.nix scope);
in

pkgs.mkShell ({
  buildInputs = definition.developmentTools;
} // definition.environmentVariables)
