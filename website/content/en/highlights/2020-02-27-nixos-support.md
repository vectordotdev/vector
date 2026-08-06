---
date: "2020-04-13"
title: "NixOS Support"
description: "Bringing Vector to NixOS"
authors: ["binarylogic"]
pr_numbers: [1946]
release: "0.9.0"
hide_on_release_notes: true
badges:
  type: "new feature"
  domains: ["platforms"]
  platforms: ["nixos"]
---

Vector is [lovingly maintained by the NixOS ecosystem][urls.vector_nix_package],
and we've now officially adopted it as a supported platform. This means we'll
produce [installation docs for the NixOS][docs.operating-systems.nixos] and
help ensure it stays up to date. We 💖 [Nixos][urls.nixos].

[docs.operating-systems.nixos]: /docs/setup/installation/operating-systems/nixos/
[urls.nixos]: https://nixos.org/
[urls.vector_nix_package]: https://github.com/NixOS/nixpkgs/blob/master/pkgs/tools/misc/vector/default.nix
