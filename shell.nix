args@{ ... }:

let
  general = (import ./default.nix {});
in

general.target.artifacts.x86_64-unknown-linux-gnu.binary