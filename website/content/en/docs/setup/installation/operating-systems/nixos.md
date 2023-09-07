---
title: Install Vector on NixOS
short: NixOS
supported_installers: ["Nix", "Docker"]
weight: 6
---

[NixOS] is a Linux distribution built on top of the Nix package manager. This
page covers installing and managing Vector on NixOS.

Nixpkgs has a [community maintained package][nixpkg-vector] for Vector. It can
be installed on a NixOS system with the following snippet in
`configuration.nix`:

```nix
environment.systemPackages = [
  pkgs.vector
];
```

See also the [Nix] package page.

## Supported installers

{{< supported-installers >}}

[nixos]: https://www.nixos.org
[nixpkg-vector]: https://github.com/NixOS/nixpkgs/tree/master/pkgs/tools/misc/vector
[nix]: /docs/setup/installation/package-managers/nix
