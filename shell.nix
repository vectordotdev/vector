args@{ pkgs ? import <nixpkgs> {} }:

let
  general = (import ./default.nix);
  definition = (import ./scripts/environment/definition.nix { inherit (general) pkgs tools; });
in

pkgs.mkShell ({
  buildInputs = definition.developmentTools;
} // definition.environmentVariables)
