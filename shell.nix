scope@{ pkgs ? import <nixpkgs> {} }:

let
  env = (import ./default.nix scope);
  definition = (import ./scripts/environment/definition.nix scope);
in

pkgs.mkShell ({
  buildInputs = [ env ];
} // definition.environmentVariables)
