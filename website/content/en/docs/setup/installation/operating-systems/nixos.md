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

This can be used to deploy and configure Vector on a NixOS system, the module
will also verify that the Vector configuration is valid before enabling any
changes.

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
        encoding = { codec = "json"; };

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

Occasionally, it'll be necessary to provide Vector with additional permissions
to access files which belong to other services. Below is an example in which
Vector is granted access to log files which belong to [Caddy][caddy]:

```nix
  services.vector = {
    enable = true;
    journaldAccess = true;
    settings = {
      sources = {
        journald.type = "journald";

        caddy = {
          type = "file";
          include = [ "/var/log/caddy/*.log" ];
        };

        vector_metrics.type = "internal_metrics";
      };

      transforms = {
        caddy_logs_timestamp = {
          type = "remap";
          inputs = [ "caddy" ];
          source = ''
            .tmp_timestamp, err = parse_json!(.message).ts * 1000000

            if err != null {
              log("Unable to parse ts value: " + err, level: "error")
            } else {
              .timestamp = from_unix_timestamp!(to_int!(.tmp_timestamp), unit: "microseconds")
            }

            del(.tmp_timestamp)
          '';
        };
      };

      sinks = {
        loki = {
          type = "loki";
          encoding.codec = "json";
          inputs = [ "caddy_logs_timestamp" "journald" ];
          endpoint = "https://loki.mycompany.com";

          labels.source = "vector";
        };

        prometheus_exporter = {
          type = "prometheus_exporter";
          inputs = [ "vector_metrics" ];
          address = "[::]:9598";
        };
      };
    };
  };

  systemd.services.vector.serviceConfig = {
    SupplementaryGroups = [ "caddy" ];
  };
```

Other integration examples may be found at the
[NixOS test suite for Vector][nixos-tests-vector]. These can also be run on a
system with Nix with a local copy of the `nixpkgs` repo by executing:

```shell
nix-build -A nixosTests.vector
```

See also the [Nix] package page.

## Supported installers

{{< supported-installers >}}

[caddy]: https://caddyserver.com
[nix]: /docs/setup/installation/package-managers/nix
[nixos]: https://www.nixos.org
[nixos-search]: https://search.nixos.org/options?query=services.vector
[nixos-tests-vector]: https://github.com/NixOS/nixpkgs/tree/master/nixos/tests/vector/
[nixpkg-vector]: https://github.com/NixOS/nixpkgs/blob/master/nixos/modules/services/logging/vector.nix
