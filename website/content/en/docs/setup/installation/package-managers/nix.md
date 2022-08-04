---
title: Nix
short: Nix
weight: 6
---

{{< info title="Community Maintained" >}}
The Vector nixpkg is supported and maintained by the open source community.
{{< /info >}}

[Nix] is a cross-platform package manager implemented on a functional deployment model where software is installed into unique directories generated through cryptographic hashes, it is also the name of the programming language. This page covers installing and managing Vector through the Nix package repository.

{{< warning title="Nix releases are typically delayed" >}}
Because Nix releases for Vector must be manually updated, expect delays between official Vector releases and release of the Nix package. New Vector packages for Nix are typically available within a few days.
{{< /warning >}}

## Installation

```shell
nix-env --install \
  --file https://github.com/NixOS/nixpkgs/archive/master.tar.gz \
  --attr vector
```

## Deployment

Vector is an end-to-end observability data pipeline designed to deploy under various roles. You mix and match these roles to create topologies. The intent is to make Vector as flexible as possible, allowing you to fluidly integrate Vector into your infrastructure over time. The deployment section demonstrates common Vector pipelines:

{{< jump "/docs/setup/deployment/topologies" >}}

## Other actions

{{< tabs default="Upgrade Vector" >}}
{{< tab title="Upgrade Vector" >}}

```shell
nix-env --upgrade vector \
  --file https://github.com/NixOS/nixpkgs/archive/master.tar.gz
```

{{< /tab >}}
{{< tab title="Uninstall Vector" >}}

```shell
nix-env --uninstall vector
```

{{< /tab >}}
{{< /tabs >}}

## Management

{{< jump "/docs/administration/management" "nix" >}}

[nix]: https://nixos.org
