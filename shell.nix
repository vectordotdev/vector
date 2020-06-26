args@{ ... }:

let
  general = (import ./default.nix);
in

general.target.artifacts.x86_64-unknown-linux-gnu.binary
# general.pkgs.libcxxStdenv.mkDerivation ({
#   name = "vector-env";
#   depsBuildHost = (general.environment.dependencies.depsBuildHost general.pkgs);
#   depsBuildBuild = (general.environment.developmentTools general.pkgs) ++ (general.environment.dependencies.depsBuildBuild general.pkgs);
#   depsHostTarget = (general.environment.dependencies.depsHostTarget general.pkgs);
#   depsHostBuild = (general.environment.dependencies.depsHostBuild general.pkgs);
#   nativeBuildInputs = (general.environment.dependencies.nativeBuildInputs general.pkgs);
  
# } // (general.environment.variables { targetPkgs = general.pkgs; hostPkgs = general.pkgs; }))
