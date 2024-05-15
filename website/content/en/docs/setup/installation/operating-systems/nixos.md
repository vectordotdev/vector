---
title: Install Vector on NixOS
short: NixOS
supported_installers: ["Nix", "Docker"]
weight: 6
---

[NixOS] is a Linux distribution built on top of the Nix package manager. This
page covers installing and managing Vector on NixOS.

Nixpkgs has a [community maintained module][nixpkg-vector] for Vector, the
options for which may be viewed on the [NixOS Search][nixos-search].

This can be used to deploy and configure Vector on a NixOS system.
For example, place into a system's `configuration.nix`:

```nix
services.vector = {
  enable = true;
  journaldAccess = true;
  settings = {
    sources = {
      journald.type = "journald";

      vector_metrics.type = "internal_metrics";
    };

    sinks = {
      loki = {
        type = "loki";
        inputs = [ "journald" ];
        endpoint = "https://loki.mycompany.com";

        labels.source = "journald";
      };

      prometheus_exporter = {
        type = "prometheus_exporter";
        inputs = [ "vector_metrics" ];
        address = "[::]:9598"
      };
    };
  };
};
```

The module will also verify that the Vector configuration is valid before
enabling any changes.

See also the [Nix] package page.

## Supported installers

{{< supported-installers >}}

[nixos]: https://www.nixos.org
[nixpkg-vector]: https://github.com/NixOS/nixpkgs/blob/master/nixos/modules/services/logging/vector.nix
[nixos-search]: https://search.nixos.org/options?query=services.vector
[nix]: /docs/setup/installation/package-managers/nix
