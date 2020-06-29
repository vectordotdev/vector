{ target ? "x86_64-unknown-linux-gnu", ... }:

let
  general = (import ./default.nix {});
in

(builtins.getAttr target general.target.artifacts).binary