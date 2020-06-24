args@{ pkgs ? import <nixpkgs> {} }:

let
  general = (import ./default.nix);
in

pkgs.libcxxStdenv.mkDerivation ({
  name = "vector-env";
  depsBuildHost = (general.environment.dependencies.depsBuildHost pkgs);
  depsBuildBuild = (general.environment.dependencies.depsBuildBuild pkgs);
  depsHostTarget = (general.environment.dependencies.depsHostTarget pkgs);
  depsHostBuild = (general.environment.dependencies.depsHostBuild pkgs);
  nativeBuildInputs = (general.environment.developmentTools pkgs) ++ (general.environment.dependencies.nativeBuildInputs pkgs);
  
} // (general.environment.variables { targetPkgs = pkgs; hostPkgs = pkgs; }))
