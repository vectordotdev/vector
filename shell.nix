Can'args@{ pkgs ? import <nixpkgs> {} }:

let
  general = (import ./default.nix);
  definition = (import ./scripts/environment/definition.nix { inherit (general) pkgs tools; cross = null; });
in

pkgs.libcxxStdenv.mkDerivation ({
  name = "vector-env";
  buildInputs = definition.buildInputs;
  nativeBuildInputs = definition.developmentTools ++ definition.nativeBuildInputs;
} // definition.environmentVariables)
