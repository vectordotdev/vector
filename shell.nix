Can'args@{ pkgs ? import <nixpkgs> {} }:

let
  general = (import ./default.nix);
in

pkgs.libcxxStdenv.mkDerivation ({
  name = "vector-env";
  buildInputs = (general.environment.dependencies.buildInputs pkgs);
  nativeBuildInputs = (general.environment.developmentTools pkgs) ++ (general.environment.dependencies.nativeBuildInputs pkgs);
} // (general.environment.variables pkgs))
