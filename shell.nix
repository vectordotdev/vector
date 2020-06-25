args@{ ... }:

let
  general = (import ./default.nix);
in

general.pkgs.libcxxStdenv.mkDerivation ({
  name = "vector-env";
  depsBuildHost = (general.environment.dependencies.depsBuildHost general.pkgs);
  depsBuildBuild = (general.environment.dependencies.depsBuildBuild general.pkgs);
  depsHostTarget = (general.environment.dependencies.depsHostTarget general.pkgs);
  depsHostBuild = (general.environment.dependencies.depsHostBuild general.pkgs);
  nativeBuildInputs = (general.environment.developmentTools general.pkgs) ++ (general.environment.dependencies.nativeBuildInputs general.pkgs);
  
} // (general.environment.variables { targetPkgs = general.pkgs; hostPkgs = general.pkgs; }))
